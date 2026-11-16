/** Fractional characters are retained so slow text does not depend on frame rate. */
export function advanceCharacters(cursor: number, elapsedMs: number, speed: number, length: number): number {
  if (speed === 0) return length;
  return Math.min(length, cursor + Math.max(0, elapsedMs) * speed / 1000);
}
