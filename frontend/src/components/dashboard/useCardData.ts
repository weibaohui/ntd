// 仪表盘新卡片通用的异步数据加载 hook。
//
// 这些新卡片(专家/Bot/版本/同步等)各自调用一个 GET 端点取数,不在 DashboardStats 聚合里,
// 因此统一封装 loading/error 三态,避免每张卡重复 useEffect 样板。
//
// cancelled 标志防御组件卸载(如切换 Tab)后的 setState,消除 React「卸载后更新」警告。
// deps 控制何时重新拉取(如依赖全局 hours 时传 [hours]);fetcher 本身不进依赖数组。
import { useEffect, useState } from 'react';

export function useCardData<T>(fetcher: () => Promise<T>, deps: ReadonlyArray<unknown> = []) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(false);
    fetcher()
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // fetcher 是内联闭包,显式进 deps 会每次 render 重建触发死循环;
    // 改由调用方把 fetcher 依赖的值放进 deps,这里只监听 deps。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { data, loading, error };
}
