import catalog from './wallpaper-catalog.json' with { type: 'json' };
import type { Wallpaper } from './types.ts';

export const builtinWallpapers: readonly Wallpaper[] = catalog;

/** Only catalogued bundled URLs bypass the scoped local asset protocol. */
export function backgroundSource(path: string | null | undefined, localAsset: (path: string) => string): string {
  if (!path) return '/background.jpg';
  return builtinWallpapers.find(item => item.path === path)?.path ?? localAsset(path);
}
