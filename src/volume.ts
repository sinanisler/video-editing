/** A point on an audio clip's volume line: `at` is frames from the clip's start, `db` the level there. */
export interface VolumePoint {
  at: number;
  db: number;
}

/**
 * The level in dB `at` frames into a clip. With no points the clip's own volume applies. Between two points the level
 * is a straight line in dB, the way a rubber band draws it. Before the first point and after the last it holds.
 */
export function volumeAt(points: VolumePoint[], volumeDb: number, at: number): number {
  if (points.length === 0) return volumeDb;
  const first = points[0];
  const last = points[points.length - 1];
  if (at <= first.at) return first.db;
  if (at >= last.at) return last.db;
  for (let i = 1; i < points.length; i++) {
    const b = points[i];
    if (at <= b.at) {
      const a = points[i - 1];
      return a.db + ((at - a.at) / (b.at - a.at)) * (b.db - a.db);
    }
  }
  return last.db;
}

/** The points with `point` added in time order. A point already at that frame is replaced. */
export function withVolumePoint(points: VolumePoint[], point: VolumePoint): VolumePoint[] {
  return [...points.filter((p) => p.at !== point.at), point].sort((x, y) => x.at - y.at);
}
