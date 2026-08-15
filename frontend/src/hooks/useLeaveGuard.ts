// useLeaveGuard.ts
// ---------------------------------------------------------------------------
// 096-W4-4-4：ProcessEditor 的「离开拦截」hook（需求 §3.6）。
//
// 承接原主组件的两层离开拦截，共享 useProcessEditorState 的 isDirty 状态
// （用户改了任何字段就置 true，保存成功后置回 false）。
//
// 两层拦截：
//  1. 路由内跳转（hash 路由）：监听 hashchange，isDirty 时弹 Modal.confirm 坝止，
//     用户确认后跳目标路由，取消则用 history.replaceState 回退旧 hash（避免再触发 hashchange）。
//  2. 刷新/关页签：window.beforeunload，isDirty 时 e.preventDefault() + returnValue=''，
//     浏览器原生提示（Chrome/Firefox/Safari 统一行为）。
//
// 注意：项目无 react-router-dom（用 hash 路由 + 自研 useViewState），
// 故 React Router 的 useBlocker 不可用，改用 window 层 hashchange 监听。
//
// 保留静态 Modal（原实现即静态）。
// ---------------------------------------------------------------------------

import { useEffect } from 'react';
import { Modal } from 'antd';

/**
 * 离开拦截：isDirty 为 true 时，路由内跳转弹确认框、刷新/关页签触发浏览器原生提示。
 * 无返回值——纯副作用 hook。
 *
 * @param isDirty 未保存修改标记（true 时才拦截）。
 * @param markClean 确认离开时清未保存标记——放行前必先清，否则设置目标 hash 会触发
 *   二次 hashchange 再次弹框（用户已确认放弃改动，不再拦截）。
 */
export function useLeaveGuard(isDirty: boolean, markClean: () => void): void {
  useEffect(() => {
    // 路由内跳转拦截：hashchange 监听
    const handleHashChange = (e: HashChangeEvent) => {
      // 仅在 isDirty 时拦截；非 dirty 放行
      if (!isDirty) return;
      // 弹 Modal.confirm 坝止跳转
      Modal.confirm({
        title: '你有未保存的修改',
        content: '确认离开？未保存的修改将丢失。',
        okText: '离开',
        cancelText: '留下',
        onOk: () => {
          // 用户确认后允许跳转：先清 isDirty=false 避免目标路由二次拦截
          // （下面设置 location.hash 会再触发一次 hashchange，不清标记会再次弹框）。
          markClean();
          // 跳目标 hash（e.newURL 已含完整 URL，取 # 后部分）。
          // 106：hash 导航在 hashchange 之前已触发原生 popstate，useViewState
          // 多半已切到目标视图；此处 hash 已等于 newHash 时赋值是 no-op，
          // 仅在异常时序（视图未同步）下补一次显式导航。
          const newHash = e.newURL.split('#')[1] ?? '';
          if (window.location.hash !== `#${newHash}`) {
            window.location.hash = newHash;
          }
        },
        onCancel: () => {
          // 取消时不跳转：hashchange 已触发，需回退旧 hash。
          // 用 history.replaceState 避免再触发 hashchange（直接设 location.hash 会再触发）。
          history.replaceState(null, '', e.oldURL);
          // 106 体检修复：replaceState 不派发任何事件，useViewState 停留在已切换的
          // 目标视图，而 URL 已回退——视图与地址脱节（编辑器已卸载、dirty 状态丢失）。
          // 补发一个合成 popstate 让 useViewState 立即按当前 URL 重新同步回编辑器。
          window.dispatchEvent(new PopStateEvent('popstate'));
        },
      });
    };
    window.addEventListener('hashchange', handleHashChange);

    // 刷新/关页签拦截：beforeunload 监听
    const handleBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!isDirty) return;
      // 标准行为：preventDefault + returnValue='' 触发浏览器原生提示
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handleBeforeUnload);

    // 清理：组件卸载或 isDirty 变化时移除监听
    return () => {
      window.removeEventListener('hashchange', handleHashChange);
      window.removeEventListener('beforeunload', handleBeforeUnload);
    };
  }, [isDirty, markClean]);
}
