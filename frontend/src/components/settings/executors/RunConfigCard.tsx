/**
 * RunConfigCard — 执行器「运行配置」卡片（096-W4-4-3 产物）。
 *
 * 从 ExecutorsPanel 拆出的全局配置族之一：最大并发数 + 执行超时。
 * 自治：挂载时拉取应用配置（getConfig），保存时回写（updateConfig）。
 *
 * 关键设计（逐字保留原实现注释）：执行超时控件脱离 Form.Item 托管，改为纯受控——
 * 展示用分钟、存储用秒，单位不同不能共用一个字段值。保存体由 handleSaveConfig 从 state 注入
 * execution_timeout_secs，保证与后端字段一致。
 */

import { useEffect, useRef, useState } from 'react';
import { App, Button, Card, Form, InputNumber, Switch, Tooltip } from 'antd';
import { InfoCircleOutlined, PlayCircleOutlined, SaveOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';
import { DEFAULT_EXECUTION_TIMEOUT_SECS, MAX_EXECUTION_TIMEOUT_MINUTES } from '@/constants';

export function RunConfigCard() {
  const { message } = App.useApp();
  // 运行配置表单：仅托管 max_concurrent_todos（超时已脱离 Form，见下）。
  const [configForm] = Form.useForm();
  const [configSaving, setConfigSaving] = useState(false);
  // 超时秒值由 state 单独持有（脱离 Form）；初始取默认值。
  const [executionTimeoutSecs, setExecutionTimeoutSecs] = useState<number>(() => DEFAULT_EXECUTION_TIMEOUT_SECS);
  // 记录「上次启用时的秒值」：关闭超时后重新开启时恢复到该值，而非 0。
  const lastEnabledExecutionTimeoutSecsRef = useRef<number>(DEFAULT_EXECUTION_TIMEOUT_SECS);

  // 0 表示禁用执行超时，其余值至少为 60 秒。
  const executionTimeoutEnabled = executionTimeoutSecs !== 0;
  // 展示用分钟（向上取整至少 1），禁用时为 undefined（InputNumber 不显示）。
  const executionTimeoutMinutes = executionTimeoutEnabled
    ? Math.max(1, Math.round(executionTimeoutSecs / 60))
    : undefined;

  /** 加载应用配置（并发数、超时等），并把后端秒值同步到超时 state。 */
  const loadConfig = async () => {
    try {
      const cfg = await db.getConfig();
      configForm.setFieldsValue(cfg);
      // 超时控件脱离 Form 托管（见 JSX），表单不再持有 execution_timeout_secs；
      // 这里手动把后端秒值同步到 state，并刷新「上次启用值」，
      // 保证 Switch/InputNumber 显示与后端一致、关闭后重开能恢复到加载值。
      const secs = cfg.execution_timeout_secs ?? DEFAULT_EXECUTION_TIMEOUT_SECS;
      setExecutionTimeoutSecs(secs);
      if (secs !== 0) {
        lastEnabledExecutionTimeoutSecsRef.current = secs;
      }
    } catch {
      // 加载失败时使用默认值（保留 state 初值）。
    }
  };

  /** 保存运行配置（并发数、超时等）。 */
  const handleSaveConfig = async () => {
    try {
      const values = await configForm.validateFields();
      // 超时控件脱离 Form 托管（见 JSX），validateFields 不再带 execution_timeout_secs；
      // 这里从 state 注入秒值，保证保存体与后端字段一致。
      values.execution_timeout_secs = executionTimeoutSecs;
      setConfigSaving(true);
      await db.updateConfig(values);
      message.success('配置已保存');
    } catch (err: unknown) {
      // antd Form.validateFields 失败时抛带 errorFields 的对象，属校验提示非真错误，静默返回。
      if (err && typeof err === 'object' && 'errorFields' in err) return;
      message.error('保存失败: ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setConfigSaving(false);
    }
  };

  /** 切换是否启用执行超时控制。 */
  const handleExecutionTimeoutToggle = (checked: boolean) => {
    if (!checked) {
      // 关闭时记录当前非零值，供后续重新开启时恢复。
      lastEnabledExecutionTimeoutSecsRef.current = executionTimeoutSecs;
    }
    const next = checked ? lastEnabledExecutionTimeoutSecsRef.current : 0;
    // 仅更新本地 state 驱动 Switch/InputNumber；保存体由 handleSaveConfig 从 state 注入。
    setExecutionTimeoutSecs(next);
  };

  // 挂载时加载配置（原主组件 useEffect 的一部分）。
  useEffect(() => {
    loadConfig();
  }, []);

  return (
    <Card
      size="small"
      title={<><PlayCircleOutlined style={{ marginRight: 6 }} />运行配置</>}
      style={{ marginTop: 16 }}
    >
      <Form form={configForm} layout="inline">
        <div style={{ display: 'flex', alignItems: 'center', gap: 24, flexWrap: 'wrap' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>最大并发数</span>
            <Form.Item name="max_concurrent_todos">
              <InputNumber
                size="small"
                min={1}
                max={20}
                style={{ width: 70 }}
              />
            </Form.Item>
            <Tooltip title="同时运行的最大 Todo 数量，超出将排队等待">
              <InfoCircleOutlined style={{ color: 'var(--color-text-quaternary)', fontSize: 12 }} />
            </Tooltip>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>执行超时</span>
            <Switch
              size="small"
              checked={executionTimeoutEnabled}
              checkedChildren="开启"
              unCheckedChildren="关闭"
              onChange={handleExecutionTimeoutToggle}
            />
            {/*
              超时控件脱离 Form.Item 托管，改为纯受控：
              展示用分钟（executionTimeoutMinutes）、存储用秒（execution_timeout_secs），
              单位不同不能共用一个字段值。此前给 Form.Item 带 name 后，antd 会用字段值
              （秒，如 300）覆盖 InputNumber 的 value，导致控件显示原始秒数而非分钟（应显示 5）。
              现在 value 取分钟、onChange 换算成秒写回 state；保存时由 handleSaveConfig
              从 state 注入 execution_timeout_secs，保存体仍然完整。
            */}
            <InputNumber
              size="small"
              min={1}
              max={MAX_EXECUTION_TIMEOUT_MINUTES}
              style={{ width: 80 }}
              disabled={!executionTimeoutEnabled}
              value={executionTimeoutMinutes}
              onChange={(v) => {
                if (v) {
                  const nextSecs = v * 60;
                  setExecutionTimeoutSecs(nextSecs);
                  lastEnabledExecutionTimeoutSecsRef.current = nextSecs;
                }
              }}
            />
            <span style={{ fontSize: 13, color: 'var(--color-text-secondary)', whiteSpace: 'nowrap' }}>分钟</span>
            <Tooltip title={`单个执行任务的最大时长（1 ~ ${MAX_EXECUTION_TIMEOUT_MINUTES} 分钟，上限 7 天）；关闭后不再因超时自动终止`}>
              <InfoCircleOutlined style={{ color: 'var(--color-text-quaternary)', fontSize: 12 }} />
            </Tooltip>
          </div>
          <Button
            size="small"
            type="primary"
            icon={<SaveOutlined />}
            loading={configSaving}
            onClick={handleSaveConfig}
          >
            保存
          </Button>
        </div>
      </Form>
    </Card>
  );
}
