import '@testing-library/jest-dom/vitest';

/**
 * Vitest 全局测试 setup。
 * 导入 jest-dom 扩展断言（如 toBeInTheDocument），供所有测试文件使用。
 */

// jsdom 未实现 ResizeObserver，而 antd 的 ResizeObserver 组件依赖它。
// 提供一个最小 polyfill，避免组件渲染时抛出 "ResizeObserver is not defined"。
class ResizeObserverPolyfill {
  observe() {}
  unobserve() {}
  disconnect() {}
}

// 全局补齐浏览器 API，仅用于测试环境
global.ResizeObserver = ResizeObserverPolyfill;
window.ResizeObserver = ResizeObserverPolyfill;

// jsdom 的 getComputedStyle 不支持第二个伪元素参数，antd 测量滚动条时会调用它。
// 这里保留原生实现并忽略伪元素参数，避免测试中打印 not implemented 错误。
const originalGetComputedStyle = window.getComputedStyle;
window.getComputedStyle = (elt: Element, pseudoElt?: string | null) => {
  if (pseudoElt) {
    return originalGetComputedStyle(elt);
  }
  return originalGetComputedStyle(elt);
};
