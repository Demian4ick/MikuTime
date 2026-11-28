import type { Settings } from './types.ts';

/** Flush in-flight operations; a rejected, already handled import must not poison later exits. */
export class AppearanceWrites {
  private tail: Promise<void> = Promise.resolve();
  private pending = new Set<Promise<void>>();
  run(write: () => Promise<void>): Promise<void> {
    const request = this.tail.then(write);
    this.tail = request.catch(() => {});
    this.pending.add(request);
    void request.then(() => this.pending.delete(request), () => this.pending.delete(request));
    return request;
  }
  async flush(): Promise<void> { await Promise.all([...this.pending]); }
}

export function applyTheme(theme: Settings['theme'], root: { dataset: { theme?: string } } = document.documentElement): void {
  const value = theme === 'Dark' ? 'dark' : 'light';
  if (root.dataset.theme !== value) root.dataset.theme = value;
}

export const wallpaperLimit = 100;
export const imageLimit = 20 * 1024 * 1024;
export const previewLimit = 256 * 1024;
export function imageDimensions(bytes: Uint8Array): { width: number; height: number } {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let width = 0, height = 0;
  if (bytes.length >= 24 && [137, 80, 78, 71, 13, 10, 26, 10].every((byte, index) => bytes[index] === byte)
      && String.fromCharCode(...bytes.slice(12, 16)) === 'IHDR') {
    width = view.getUint32(16); height = view.getUint32(20);
  } else if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    let offset = 2;
    while (offset < bytes.length && bytes[offset] === 0xff) {
      while (bytes[offset] === 0xff) offset++;
      const marker = bytes[offset++];
      if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) continue;
      if (marker === 0xda || marker === 0xd9 || offset + 2 > bytes.length) break;
      const length = view.getUint16(offset);
      if (length < 2 || offset + length > bytes.length) break;
      if ([0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf].includes(marker)) {
        if (length < 8) break;
        height = view.getUint16(offset + 3); width = view.getUint16(offset + 5); break;
      }
      offset += length;
    }
  }
  if (!width || !height) throw new Error('Не удалось прочитать изображение. Выберите PNG или JPEG.');
  if (width > 4096 || height > 4096) throw new Error('Изображение должно быть не больше 4096×4096.');
  return { width, height };
}
export function wallpaperName(fileName: string): string {
  return Array.from(fileName.split(/[\\/]/).at(-1)!.replace(/\.[^.]+$/, '').replace(/[\u0000-\u001f\u007f-\u009f]/g, '').trim()).slice(0, 80).join('') || 'Обои';
}
export function encodeWallpaper(name: string, image: Uint8Array, preview: Uint8Array): Uint8Array {
  if (!image.length || image.length > imageLimit) throw new Error('Файл обоев должен быть не больше 20 МиБ.');
  if (!preview.length || preview.length > previewLimit) throw new Error('Не удалось подготовить миниатюру.');
  const metadata = new TextEncoder().encode(JSON.stringify({ name, previewLength: preview.length }));
  if (metadata.length > 4096) throw new Error('Слишком длинное название обоев.');
  const bytes = new Uint8Array(4 + metadata.length + preview.length + image.length);
  new DataView(bytes.buffer).setUint32(0, metadata.length, true);
  bytes.set(metadata, 4); bytes.set(preview, 4 + metadata.length); bytes.set(image, 4 + metadata.length + preview.length);
  return bytes;
}

/** Decode one image at a time; the gallery uses small persisted previews, not full-size originals. */
export async function prepareWallpaper(file: File): Promise<Uint8Array> {
  if (!file.size || file.size > imageLimit) throw new Error('Файл обоев должен быть не больше 20 МиБ.');
  const bytes = new Uint8Array(await file.arrayBuffer());
  imageDimensions(bytes); // Reject oversized headers before allocating a decoded bitmap.
  let bitmap: ImageBitmap;
  try { bitmap = await createImageBitmap(file); } catch { throw new Error('Не удалось прочитать изображение. Выберите PNG или JPEG.'); }
  const canvas = document.createElement('canvas');
  try {
    if (!bitmap.width || !bitmap.height || bitmap.width > 4096 || bitmap.height > 4096) throw new Error('Изображение должно быть не больше 4096×4096.');
    canvas.width = 320; canvas.height = 180;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Не удалось создать миниатюру.');
    context.fillStyle = '#e4fcf5'; context.fillRect(0, 0, 320, 180);
    const scale = Math.max(320 / bitmap.width, 180 / bitmap.height);
    context.drawImage(bitmap, (320 - bitmap.width * scale) / 2, (180 - bitmap.height * scale) / 2, bitmap.width * scale, bitmap.height * scale);
    const preview = await new Promise<Blob>((resolve, reject) => canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error('Не удалось создать миниатюру.')), 'image/jpeg', 0.8));
    return encodeWallpaper(wallpaperName(file.name), bytes, new Uint8Array(await preview.arrayBuffer()));
  } finally { bitmap.close(); canvas.width = 0; canvas.height = 0; }
}
