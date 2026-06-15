/**
 * 剪贴板操作工具函数
 *
 * 升级到使用 clipboard.js 实现，统一处理 HTTPS/HTTP 环境下的复制问题：
 * - HTTPS / localhost：clipboard.js 内部走 navigator.clipboard.writeText
 * - HTTP（非安全上下文）：clipboard.js 自动 fallback 到 document.execCommand('copy')
 * - 无需业务侧关心安全上下文，统一通过 Promise<boolean> 返回结果
 *
 * 对应 Issue #599：前端页面点击复制功能升级
 */

import ClipboardJS from 'clipboard';

/**
 * 复制文本到剪贴板
 *
 * 内部封装 clipboard.js：每次调用创建一个不可见的临时按钮进行绑定与触发，
 * 在 success/error 事件或超时后销毁实例并移除临时按钮，避免内存泄漏与重复触发。
 *
 * @param text 要复制的文本
 * @returns Promise<boolean> 复制是否成功
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  // clipboard.js 在不支持的环境下（例如极老浏览器）可直接判定失败，避免创建多余 DOM
  if (!ClipboardJS.isSupported()) {
    console.warn('当前环境不支持剪贴板写入');
    return false;
  }

  return new Promise<boolean>((resolve) => {
    // 创建不可见的临时按钮：clipboard.js 必须绑定到真实 DOM 节点才能触发 click
    // 使用 fixed + 负坐标定位到屏幕外，避免影响页面布局与滚动
    const button = document.createElement('button');
    button.type = 'button';
    button.setAttribute('aria-hidden', 'true');
    button.style.position = 'fixed';
    button.style.top = '-9999px';
    button.style.left = '-9999px';
    button.style.width = '1px';
    button.style.height = '1px';
    button.style.padding = '0';
    button.style.margin = '0';
    button.style.border = '0';
    button.style.opacity = '0';
    button.style.pointerEvents = 'none';

    let settled = false;
    // 用闭包统一处理成功 / 失败 / 超时，避免重复 resolve 与泄漏临时节点
    const settle = (ok: boolean) => {
      if (settled) return;
      settled = true;
      clipboard.destroy();
      if (button.parentNode === document.body) {
        document.body.removeChild(button);
      }
      resolve(ok);
    };

    // 绑定 clipboard.js 到临时按钮，text 回调动态返回本次要复制的文本
    const clipboard = new ClipboardJS(button, {
      text: () => text,
    });

    clipboard.on('success', () => settle(true));
    clipboard.on('error', () => settle(false));

    // 必须先挂载到 DOM 再点击，clipboard.js 内部依赖真实的 click 事件冒泡
    document.body.appendChild(button);
    button.click();

    // 兜底超时：极端情况下 success/error 都不触发也要 resolve 并清理资源
    setTimeout(() => settle(false), 1000);
  });
}
