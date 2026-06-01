import { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Modal, Form, Select, Switch, message,
  Card, Row, Col, Table, Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { getTodoHookConfig, updateTodoHookConfig, getHooks } from '../utils/database';
import type { HookRule } from '../utils/database/hooks';
import { HOOK_MODES } from '../utils/database/hooks';

const { Text } = Typography;

interface TodoHooksModalProps {
  open: boolean;
  todoId: number;
  onClose: () => void;
}

interface TodoHookConfig {
  todo_id: number;
  hook_mode: string;
  override_enabled: boolean;
  rule_ids: number[];
}

export function TodoHooksModal({ open, todoId, onClose }: TodoHooksModalProps) {
  const [config, setConfig] = useState<TodoHookConfig | null>(null);
  const [allHooks, setAllHooks] = useState<HookRule[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [selectedRuleIds, setSelectedRuleIds] = useState<number[]>([]);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [configData, hooksData] = await Promise.all([
        getTodoHookConfig(todoId).catch(() => null),
        getHooks(),
      ]);
      const effectiveConfig = configData || {
        todo_id: todoId,
        hook_mode: 'inherit',
        override_enabled: false,
        rule_ids: [],
      };
      setConfig(effectiveConfig);
      setAllHooks(hooksData.filter(h => h.enabled));
      setSelectedRuleIds(effectiveConfig.rule_ids || []);
    } catch (e: any) {
      message.error('加载失败: ' + e.message);
    } finally {
      setLoading(false);
    }
  }, [todoId]);

  useEffect(() => {
    if (!open || !todoId) return;
    loadData();
  }, [open, todoId, loadData]);

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await updateTodoHookConfig(todoId, {
        hook_mode: config.hook_mode,
        override_enabled: config.override_enabled,
        rule_ids: selectedRuleIds,
      });
      message.success('保存成功');
      onClose();
    } catch (e: any) {
      message.error('保存失败: ' + e.message);
    } finally {
      setSaving(false);
    }
  };

  const handleRuleToggle = useCallback((ruleId: number, checked: boolean) => {
    setSelectedRuleIds(prev =>
      checked ? [...prev, ruleId] : prev.filter(id => id !== ruleId)
    );
  }, []);

  const hookTableColumns = useMemo<ColumnsType<HookRule>>(() => [
    { title: '名称', dataIndex: 'name', key: 'name' },
    {
      title: '触发器',
      dataIndex: 'trigger',
      key: 'trigger',
      render: (t: string) => {
        const trigger = allHooks.find(h => h.trigger === t);
        return trigger ? t : <Text type="secondary">{t}</Text>;
      },
    },
    {
      title: '已选择',
      key: 'selected',
      render: (_, record) => (
        <Switch
          checked={selectedRuleIds.includes(record.id)}
          onChange={(checked) => handleRuleToggle(record.id, checked)}
        />
      ),
    },
  ], [allHooks, selectedRuleIds, handleRuleToggle]);

  if (!config) return null;

  return (
    <Modal
      title="Hook 配置"
      open={open}
      onCancel={onClose}
      onOk={handleSave}
      width={700}
      confirmLoading={saving}
      okText="保存"
      cancelText="取消"
    >
      <Form layout="vertical">
        <Row gutter={16}>
          <Col span={12}>
            <Form.Item label="Hook 模式">
              <Select
                value={config.hook_mode}
                onChange={(v) => setConfig({ ...config, hook_mode: v })}
              >
                {HOOK_MODES.map(m => (
                  <Select.Option key={m.value} value={m.value}>{m.label}</Select.Option>
                ))}
              </Select>
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item label="覆盖启用">
              <Switch
                checked={config.override_enabled}
                onChange={(v) => setConfig({ ...config, override_enabled: v })}
              />
              <Text type="secondary" style={{ marginLeft: 8 }}>
                {config.override_enabled ? '已启用' : '未启用'}
              </Text>
            </Form.Item>
          </Col>
        </Row>

        {config.hook_mode === 'inherit' && (
          <Card size="small" style={{ marginBottom: 16 }}>
            <Text type="secondary">
              继承全局默认 Hook。可以在「配置管理 - Hooks」中设置全局默认规则。
            </Text>
          </Card>
        )}

        {config.hook_mode === 'disabled' && (
          <Card size="small" style={{ marginBottom: 16 }}>
            <Text type="secondary">
              此 Todo 的 Hook 功能已禁用。
            </Text>
          </Card>
        )}

        {config.hook_mode === 'custom' && (
          <>
            <Text strong style={{ display: 'block', marginBottom: 8 }}>
              选择要使用的 Hook 规则：
            </Text>
            <Table
              columns={hookTableColumns}
              dataSource={allHooks}
              rowKey="id"
              size="small"
              pagination={false}
              loading={loading}
              style={{ marginBottom: 16 }}
            />
            <Text type="secondary">
              已选择 {selectedRuleIds.length} 个规则
            </Text>
          </>
        )}
      </Form>
    </Modal>
  );
}
