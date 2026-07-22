import { Switch } from 'antd';
import { useConsolePanel } from '@/hooks/useConsolePanel';

/**
 * 界面显示设置面板。
 * 暂只放底部全局执行日志面板的显隐开关，后续如有其他纯前端 UI 偏好（如列表密度等）可在此扩展。
 */
export function InterfaceDisplayPanel() {
  const { visible, setVisible } = useConsolePanel();

  return (
    <div style={{ width: '100%' }}>
      {/* 关闭后即使有运行中的任务也不会弹出底部日志框，适合不想被面板打扰的场景；
          日志数据仍在 state.runningTasks 中正常累积，重新打开即可立刻看到期间的日志。 */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span>显示底部执行日志面板</span>
        <Switch checked={visible} onChange={setVisible} />
      </div>
    </div>
  );
}
