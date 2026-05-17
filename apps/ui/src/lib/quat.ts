/**
 * Convert a unit quaternion (XYZW) into Euler angles (roll, pitch, yaw)
 * in degrees. Convention: ZYX intrinsic (yaw → pitch → roll).
 *
 * Matches the convention used by SlimeVR-Server display panels so the
 * numbers shown here line up with the server-side rendering.
 */
export function quatToEulerDeg(q: [number, number, number, number]): {
  roll: number;
  pitch: number;
  yaw: number;
} {
  const [x, y, z, w] = q;
  const sinrCosp = 2 * (w * x + y * z);
  const cosrCosp = 1 - 2 * (x * x + y * y);
  const roll = Math.atan2(sinrCosp, cosrCosp);

  const sinp = 2 * (w * y - z * x);
  const pitch = Math.abs(sinp) >= 1 ? Math.sign(sinp) * (Math.PI / 2) : Math.asin(sinp);

  const sinyCosp = 2 * (w * z + x * y);
  const cosyCosp = 1 - 2 * (y * y + z * z);
  const yaw = Math.atan2(sinyCosp, cosyCosp);

  const k = 180 / Math.PI;
  return { roll: roll * k, pitch: pitch * k, yaw: yaw * k };
}
