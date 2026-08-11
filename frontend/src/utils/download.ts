/**
 * 浏览器下载工具（096-W1-PR4 收口产物）。
 *
 * 原 BackupPanel 里散着 5 份逐字同构的下载套路：
 * - 3 份「URL → 临时 <a> 点击」骨架（database/todo/skill 备份文件下载）
 * - 2 份「fetch → Blob → objectURL → <a> 点击 → revoke」骨架（数据库下载、YAML 导出）
 * 任何交互细节调整（如命名策略、回收时机）都要同步改多处，这里收敛为两个原语。
 */

/**
 * 生成备份文件名用的时间戳片段。
 *
 * ISO 串含 `:` 与 `.`，直接进文件名在 Windows 上非法，故替换为 `-`；
 * 截断到秒级（19 字符）兼顾可读性与唯一性——与原实现逐字保持同一格式。
 */
export function backupTimestamp(now: Date = new Date()): string {
  return now.toISOString().replace(/[:.]/g, '-').slice(0, 19);
}

/**
 * 触发浏览器下载：适用于「同源 GET 链接」场景（服务端流式返回文件）。
 *
 * 通过临时 <a download> 元素触发浏览器原生下载，不经过 axios 拦截器；
 * 元素即插即拔，不残留 DOM 垃圾。
 */
export function downloadByUrl(url: string, filename: string): void {
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

/**
 * 触发浏览器下载：适用于「fetch 已在内存里拿到内容」的场景（Blob/文本）。
 *
 * objectURL 在 click 后立即 revoke——与原实现保持同一回收时机；
 * 用 try/finally 兜底，即使 downloadByUrl 抛错也不泄漏 objectURL。
 */
export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  try {
    downloadByUrl(url, filename);
  } finally {
    URL.revokeObjectURL(url);
  }
}
