/**
 * UUID v4 生成工具。
 *
 * 背景：`crypto.randomUUID()` 仅在 secure context（https 或 localhost）可用。当通过
 * 非 localhost 的 IP 直连访问开发/测试服务（如 http://192.168.x.x:18088）时，
 * `crypto.randomUUID` 不存在，「创建工艺」等需要即时生成 guid 的场景会报
 * `crypto.randomUUID is not a function`。这里做降级：secure context 走原生 API，
 * 否则用 `crypto.getRandomValues`（所有 http/https context 均可用）手搓 RFC 4122 v4。
 */

/**
 * 按 RFC 4122 把 16 字节随机数格式化为 v4 UUID。
 *
 * 抽成纯函数便于单测：给定固定字节可断言 version/variant 置位正确，无需 mock crypto。
 * 复制一份字节再改——`getRandomValues` 可能复用调用方传入的缓冲区，避免副作用。
 */
export function bytesToUUIDv4(bytes: Uint8Array): string {
  const b = bytes.slice();
  // 版本号位：高 4 位置为 0100（v4）
  b[6] = (b[6] & 0x0f) | 0x40;
  // variant 位：高 2 位置为 10（RFC 4122 变体）
  b[8] = (b[8] & 0x3f) | 0x80;
  const hex = Array.from(b, (x) => x.toString(16).padStart(2, '0'));
  return `${hex.slice(0, 4).join('')}-${hex.slice(4, 6).join('')}-${hex.slice(6, 8).join('')}-${hex.slice(8, 10).join('')}-${hex.slice(10, 16).join('')}`;
}

/**
 * 生成一个 RFC 4122 v4 UUID。
 *
 * 优先 `crypto.randomUUID`（secure context，原生最快最稳）；不可用时回退
 * `bytesToUUIDv4(crypto.getRandomValues(...))`，覆盖 IP 直连等非 secure context。
 */
export function generateUUID(): string {
  // secure context（https / localhost）：原生 randomUUID
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  // 非 secure context（http + IP）：getRandomValues 仍可用，手搓 v4
  return bytesToUUIDv4(crypto.getRandomValues(new Uint8Array(16)));
}
