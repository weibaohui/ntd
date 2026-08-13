/**
 * ExecutorsTable — 执行器 tab 的列表表格区（096-W4-4-3 产物）。
 *
 * 从 ExecutorsPanel 拆出的最大一块：执行器表（7 列）+ 批量检测 + 测试结果 Modal +
 * 运行配置/AI 使用统计两张卡片。主组件只剩 Tabs 编排，本组件承接「执行器列表交互」单一职责。
 *
 * 列 render 原在主组件内联成两段 >50 行函数（默认模型列、操作列），现各拆为命名 helper：
 * - renderModelCell：supports_models 分支（Input 行内保存 / Select 分组下拉）+ groupModelsByProvider 纯数据构建；
 * - renderActions：行操作按钮组 + renderInstallButton（安装入口随检测结果条件出现）。
 * 每个 helper 控制在 50 行内，列数组本身是纯配置字面量。
 */

import { Button, Input, Select, Switch, Spin, Tooltip, Modal, Typography, Table, Space, Empty } from 'antd';
import { SearchOutlined, PlayCircleOutlined, BugOutlined, StarOutlined, StarFilled } from '@ant-design/icons';
import { InstallExecutorButton } from '@/components/settings/InstallExecutorButton';
import { getExecutorInstallPrompt } from '@/components/settings/executorInstallPrompts';
import { RunConfigCard } from '@/components/settings/executors/RunConfigCard';
import { UsageStatsCard } from '@/components/settings/executors/UsageStatsCard';
import type { ExecutorConfig } from '@/types';
import type { UseExecutorAdminReturn } from '@/hooks/useExecutorAdmin';
import type { UseExecutorFieldSaverReturn } from '@/hooks/useExecutorFieldSaver';

const { Paragraph } = Typography;

/**
 * 把「provider/model」全名按 provider 分组，供 Select.OptGroup 展示。
 * 无斜杠的归入「其他」；纯数据构建，从 renderModelCell 抽出以压低其行数。
 */
function groupModelsByProvider(models: string[]): Record<string, { label: string; value: string }[]> {
  const groups: Record<string, { label: string; value: string }[]> = {};
  models.forEach((full) => {
    const slash = full.indexOf('/');
    const provider = slash > 0 ? full.slice(0, slash) : '其他';
    const modelName = slash > 0 ? full.slice(slash + 1) : full;
    if (!groups[provider]) groups[provider] = [];
    groups[provider].push({ label: modelName, value: full });
  });
  return groups;
}

export function ExecutorsTable({ admin, saver }: { admin: UseExecutorAdminReturn; saver: UseExecutorFieldSaverReturn }) {
  // enabled 开关属 onChange 型保存（非 blur），直接调 saveExecutorField。
  const renderEnabledSwitch = (record: ExecutorConfig) => (
    <Switch
      size="small"
      checked={record.enabled}
      loading={saver.savingExecutor === record.name}
      onChange={(checked) => { void saver.saveExecutorField(record.name, { enabled: checked }); }}
    />
  );

  const renderNameCell = (name: string, record: ExecutorConfig) => (
    <span style={{ fontWeight: 500, opacity: record.enabled ? 1 : 0.5, display: 'inline-flex', alignItems: 'center', gap: 6 }}>
      {name}
      {record.is_default && (
        <Tooltip title="默认执行器">
          <StarFilled style={{ color: '#faad14', fontSize: 12 }} />
        </Tooltip>
      )}
    </span>
  );

  // Input 型行内保存：inlineFieldSave 封装 blur/Enter 双触发 + 未改不存。
  // 改路径后顺手清检测状态（旧结果随路径失效）。
  const renderPathCell = (path: string, record: ExecutorConfig) => {
    const save = saver.inlineFieldSave(record.name, path, async (newPath) => {
      const updated = await saver.saveExecutorField(record.name, { path: newPath });
      if (updated) admin.clearDetectResult(record.name);
    });
    return (
      <Input size="small" placeholder="二进制路径或命令名" defaultValue={path} onBlur={save.onBlur} onPressEnter={save.onPressEnter} />
    );
  };

  const renderSessionDirCell = (sessionDir: string, record: ExecutorConfig) => {
    const save = saver.inlineFieldSave(record.name, sessionDir, async (newDir) => {
      await saver.saveExecutorField(record.name, { session_dir: newDir });
    });
    return (
      <Input size="small" placeholder="如 ~/.claude" defaultValue={sessionDir} onBlur={save.onBlur} onPressEnter={save.onPressEnter} />
    );
  };

  // 不支持列模型的执行器：普通 Input 行内保存（留空 = 不传 --model，向后兼容）。
  const renderModelInput = (defaultModel: string, record: ExecutorConfig) => {
    const save = saver.inlineFieldSave(record.name, defaultModel, async (newModel) => {
      await saver.saveExecutorField(record.name, { default_model: newModel });
    });
    return (
      <Input size="small" placeholder="留空用执行器自带配置" defaultValue={defaultModel} onBlur={save.onBlur} onPressEnter={save.onPressEnter} />
    );
  };

  // supports_models：Select 下拉，按 provider 分组展示后端列出的模型。
  const renderModelSelect = (defaultModel: string | null | undefined, record: ExecutorConfig) => {
    const models = admin.executorModels[record.name] || [];
    const groups = groupModelsByProvider(models);
    return (
      <Select
        size="small"
        value={defaultModel || undefined}
        placeholder="留空用执行器自带配置"
        allowClear
        showSearch
        notFoundContent={
          admin.modelsLoading[record.name] ? '加载中，请稍后...' : models.length === 0 ? '暂无可选模型' : undefined
        }
        onDropdownVisibleChange={(open) => admin.handleModelsDropdown(record.name, open)}
        filterOption={(input: string, option?: { label: string; value: string }) =>
          (option?.label ?? '').toLowerCase().includes(input.toLowerCase())}
        onChange={(v: unknown) => {
          const newModel = (v as string)?.trim() || '';
          // 未改动不保存（避免清空即触发无谓请求）。
          if (newModel === (defaultModel ?? '')) return;
          void saver.saveExecutorField(record.name, { default_model: newModel });
        }}
        style={{ width: '100%' }}
      >
        {Object.entries(groups).map(([provider, items]) => (
          <Select.OptGroup key={provider} label={provider}>
            {items.map((item) => (
              <Select.Option key={item.value} value={item.value}>{item.label}</Select.Option>
            ))}
          </Select.OptGroup>
        ))}
      </Select>
    );
  };

  // 默认模型列：按 supports_models 分流到 Input（行内保存）或 Select（分组下拉）。
  const renderModelCell = (defaultModel: string | null | undefined, record: ExecutorConfig) => {
    if (!record.supports_models) return renderModelInput(defaultModel ?? '', record);
    return renderModelSelect(defaultModel, record);
  };

  const renderDetectStatus = (record: ExecutorConfig) => {
    const detectResult = admin.detectResults[record.name];
    if (!detectResult) {
      return <span style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>未检测</span>;
    }
    return (
      <Tooltip title={detectResult.resolved || '未找到'}>
        {detectResult.found ? (
          <span style={{ color: '#52c41a', fontSize: 12, fontWeight: 500 }}>✓ 可用</span>
        ) : (
          <span style={{ color: '#ff4d4f', fontSize: 12, fontWeight: 500 }}>✗ 不可用</span>
        )}
      </Tooltip>
    );
  };

  // 安装入口：仅当检测结果为「不可用」且该执行器有安装提示时出现。
  // 安装完成后走 refreshAfterInstall（detect+repair+updateExecutor 三步兜底链）。
  const renderInstallButton = (record: ExecutorConfig) => {
    const installPrompt = getExecutorInstallPrompt(record.name);
    if (!installPrompt) return null;
    return (
      <InstallExecutorButton
        executorName={record.name}
        displayName={record.display_name}
        prompt={installPrompt.prompt}
        buttonSize="small"
        showLabel={true}
        onInstalled={() => { void admin.refreshAfterInstall(record); }}
      />
    );
  };

  // 行操作：设为默认 / 检测 / 修复（不可用时）/ 安装（有提示时）/ 测试。
  const renderActions = (record: ExecutorConfig) => {
    const detectResult = admin.detectResults[record.name];
    return (
      <Space size={4}>
        <Tooltip title={record.is_default ? '当前为默认执行器' : '设为默认执行器'}>
          <Button
            size="small"
            type={record.is_default ? 'primary' : 'default'}
            icon={record.is_default ? <StarFilled /> : <StarOutlined />}
            loading={admin.settingDefaultExecutor === record.name}
            disabled={record.is_default}
            onClick={() => { void admin.setAsDefault(record); }}
          >
            {record.is_default ? '默认' : '设为默认'}
          </Button>
        </Tooltip>
        <Button
          size="small"
          icon={<SearchOutlined />}
          loading={admin.detectingExecutor === record.name}
          onClick={() => { void admin.detectExecutorByName(record); }}
        >
          检测
        </Button>
        {!detectResult?.found && (
          <Button size="small" icon={<BugOutlined />} onClick={() => { void admin.repairByName(record); }}>
            修复
          </Button>
        )}
        {detectResult && !detectResult.found && renderInstallButton(record)}
        <Button
          size="small"
          type="primary"
          ghost
          icon={<PlayCircleOutlined />}
          loading={admin.testingExecutor === record.name}
          onClick={() => { void admin.testExecutorByName(record); }}
        >
          测试
        </Button>
      </Space>
    );
  };

  // 列定义：纯配置字面量，render 各委托给上面的命名 helper（单列逻辑 ≤50 行）。
  const columns = [
    { title: '状态', dataIndex: 'enabled', key: 'enabled', align: 'center' as const, render: (_: unknown, r: ExecutorConfig) => renderEnabledSwitch(r) },
    { title: '执行器', dataIndex: 'display_name', key: 'display_name', render: (v: string, r: ExecutorConfig) => renderNameCell(v, r) },
    { title: '二进制路径', dataIndex: 'path', key: 'path', render: (v: string, r: ExecutorConfig) => renderPathCell(v, r) },
    { title: 'Session 目录', dataIndex: 'session_dir', key: 'session_dir', render: (v: string, r: ExecutorConfig) => renderSessionDirCell(v, r) },
    {
      // 默认模型：执行器级默认，所有未单独指定模型的 todo 用该执行器时默认传此模型。
      title: '默认模型', dataIndex: 'default_model', key: 'default_model', render: (v: string | null | undefined, r: ExecutorConfig) => renderModelCell(v, r),
    },
    { title: '检测状态', key: 'detect_status', align: 'center' as const, render: (_: unknown, r: ExecutorConfig) => renderDetectStatus(r) },
    { title: '操作', key: 'action', width: 240, render: (_: unknown, r: ExecutorConfig) => renderActions(r) },
  ];

  // Modal 标题里的执行器显示名兜底：testModalData 只存 name，回查列表取 display_name。
  const testTitle = admin.testModalData
    ? `测试结果 - ${admin.executors.find((e) => e.name === admin.testModalData?.name)?.display_name || admin.testModalData?.name}`
    : '测试结果';

  return (
    <Spin spinning={admin.executorsLoading}>
      <div>
        <Paragraph type="secondary" style={{ marginBottom: 16 }}>
          管理执行器的路径、开关状态，并检测二进制是否可用。关闭开关的执行器不会出现在 Todo 的执行器选择列表中。
        </Paragraph>
        <div style={{ marginBottom: 12, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ color: 'var(--color-text-secondary)', fontSize: 13 }}>共 {admin.executors.length} 个执行器</span>
          <Button type="primary" icon={<SearchOutlined />} loading={admin.batchDetecting} onClick={() => { void admin.batchDetect(); }}>
            批量检测
          </Button>
        </div>

        <Table
          rowKey="name"
          dataSource={admin.executors}
          pagination={false}
          size="middle"
          locale={{ emptyText: <Empty description="暂无执行器" image={Empty.PRESENTED_IMAGE_SIMPLE} /> }}
          columns={columns}
        />

        {/* 运行配置 / AI 使用统计两块全局配置已拆为独立卡片子组件，各自挂载即加载。 */}
        <RunConfigCard />
        <UsageStatsCard />

        {/* 执行器测试结果 Modal */}
        <Modal title={testTitle} open={admin.testModalVisible} onCancel={admin.closeTestModal} footer={<Button onClick={admin.closeTestModal}>关闭</Button>} width={500}>
          {admin.testModalData && (
            <div>
              <p>
                状态：{admin.testModalData.result.test_passed
                  ? <span style={{ color: '#52c41a', fontWeight: 600 }}>通过</span>
                  : <span style={{ color: '#ff4d4f', fontWeight: 600 }}>失败</span>}
              </p>
              {admin.testModalData.result.error && (
                <p style={{ color: '#ff4d4f' }}>错误：{admin.testModalData.result.error}</p>
              )}
              {admin.testModalData.result.output && (
                <div>
                  <Paragraph type="secondary">输出：</Paragraph>
                  <pre style={{
                    background: 'var(--color-bg-container)', color: 'var(--color-text-secondary)',
                    padding: 12, borderRadius: 6, fontSize: 12, maxHeight: 300, overflow: 'auto', whiteSpace: 'pre-wrap', margin: 0,
                  }}>
                    {admin.testModalData.result.output}
                  </pre>
                </div>
              )}
            </div>
          )}
        </Modal>
      </div>
    </Spin>
  );
}
