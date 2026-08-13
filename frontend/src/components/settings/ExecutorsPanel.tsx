import { Tabs } from 'antd';
import { CodeOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import { ProfilesPanel } from '@/components/settings/ProfilesPanel';
import { ExecutorsTable } from '@/components/settings/executors/ExecutorsTable';
import { RunningRecordsTable } from '@/components/settings/executors/RunningRecordsTable';
import { SessionManager } from '@/components/SessionManager';
import { useExecutorAdmin } from '@/hooks/useExecutorAdmin';
import { useExecutorFieldSaver } from '@/hooks/useExecutorFieldSaver';
import { useRunningRecords } from '@/hooks/useRunningRecords';

/**
 * 执行器管理面板（096-W4-4-3 拆分后）：纯 Tabs 编排层。
 *
 * 三族状态由 hook 托管，两张表格分别下沉到子组件，本组件只组装 Tabs：
 * - useExecutorAdmin：列表 + 检测/测试/修复/设默认/安装刷新 + 模型缓存
 * - useExecutorFieldSaver：行内字段保存收敛（saveExecutorField + inlineFieldSave）
 * - useRunningRecords：运行监控族 + 面板 Tab 状态
 * - ExecutorsTable：执行器配置表（执行器 tab）
 * - RunningRecordsTable：运行中执行记录表（正在运行 tab）
 */
export function ExecutorsPanel() {
  // 三族状态分别由 hook 托管；saver 需要 admin 的 replaceExecutor 把保存结果回写列表。
  const admin = useExecutorAdmin();
  const saver = useExecutorFieldSaver(admin.replaceExecutor);
  // 运行监控族依赖执行器列表（派生 name→display_name 映射）。
  const running = useRunningRecords(admin.executors);

  return (
    <PageCard icon={<CodeOutlined />} title="执行器">
      <Tabs
        activeKey={running.runningTab}
        onChange={(key) => running.setRunningTab(key as 'executors' | 'running')}
        items={[
          { key: 'executors', label: '执行器', children: <ExecutorsTable admin={admin} saver={saver} /> },
          { key: 'api-key', label: 'API Key', children: <ProfilesPanel /> },
          { key: 'running', label: '正在运行', children: <RunningRecordsTable running={running} /> },
          { key: 'sessions', label: '会话', children: <SessionManager embedded /> },
        ]}
      />
    </PageCard>
  );
}
