export function macHex(mac: number[] | Uint8Array): string {
  return Array.from(mac)
    .map((b) => b.toString(16).padStart(2, "0").toUpperCase())
    .join(":");
}

export function macKey(mac: number[] | Uint8Array): string {
  return Array.from(mac)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
