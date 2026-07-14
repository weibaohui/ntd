import { useState, useMemo, useCallback } from 'react';
import { Button, Space, Input, Typography, message, Tag } from 'antd';
import { CheckOutlined, EditOutlined, RocketOutlined } from '@ant-design/icons';
import * as db from '@/utils/database';

const { Text, Paragraph } = Typography;
const { TextArea } = Input;

interface ExpertCreateCompletedProps {
  result: string;
  close: () => void;
  onCreated: () => void;
}

/**
 * AI 创建专家完成态组件。
 *
 * 解析 AI 返回的结果，提取 plugin_json 和 agent_md，展示预览并允许用户编辑，
 * 用户确认后调用后端 API 创建专家。
 */
export function ExpertCreateCompleted({ result, close, onCreated }: ExpertCreateCompletedProps) {
  // 解析 AI 输出，提取 plugin_json 和 agent_md
  const parseResult = useMemo(() => {
    const jsonMatch = result.match(/```json\s*([\s\S]*?)\s*```/);
    const mdMatch = result.match(/```markdown\s*([\s\S]*?)\s*```/);
    
    return {
      pluginJson: jsonMatch ? jsonMatch[1].trim() : '',
      agentMd: mdMatch ? mdMatch[1].trim() : '',
      raw: !jsonMatch || !mdMatch,
    };
  }, [result]);

  const [pluginJson, setPluginJson] = useState(parseResult.pluginJson);
  const [agentMd, setAgentMd] = useState(parseResult.agentMd);
  const [creating, setCreating] = useState(false);

  // 如果 JSON 无效，尝试解析看看
  const pluginPreview = useMemo(() => {
    try {
      return JSON.parse(pluginJson);
    } catch {
      return null;
    }
  }, [pluginJson]);

  const handleCreate = useCallback(async () => {
    if (!pluginJson.trim() || !agentMd.trim()) {
      message.warning('请填写完整的专家信息');
      return;
    }

    // 验证 JSON 格式
    try {
      JSON.parse(pluginJson);
    } catch {
      message.error('plugin.json 格式无效');
      return;
    }

    setCreating(true);
    try {
      await db.createExpertFromAi(pluginJson, agentMd);
      message.success('专家创建成功');
      onCreated();
      close();
    } catch (err: any) {
      message.error('创建失败: ' + (err?.message || String(err)));
    } finally {
      setCreating(false);
    }
  }, [pluginJson, agentMd, close, onCreated]);

  // AI 输出无法解析时展示原始内容
  if (parseResult.raw) {
    return (
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <Text type="warning">AI 输出格式不符合要求，请重试</Text>
        <div style={{ padding: 12, background: 'var(--color-bg-elevated)', borderRadius: 6, maxHeight: 400, overflow: 'auto' }}>
          <Paragraph style={{ whiteSpace: 'pre-wrap', margin: 0 }}>
            {result}
          </Paragraph>
        </div>
      </Space>
    );
  }

  return (
    <Space direction="vertical" size="middle" style={{ width: '100%' }}>
      {/* 提示信息 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <CheckOutlined style={{ color: 'var(--color-success)', fontSize: 18 }} />
        <Text type="secondary">AI 已生成专家定义，确认无误后点击创建</Text>
      </div>

      {/* 专家基本信息预览 */}
      {pluginPreview && (
        <div style={{ padding: 12, background: 'var(--color-success-bg-1)', borderRadius: 8, border: '1px solid var(--color-success-border)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <Tag color="blue">{pluginPreview.expertType}</Tag>
            <Text strong>{pluginPreview.displayName?.zh || pluginPreview.name}</Text>
          </div>
          <div style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>
            {pluginPreview.profession?.zh || pluginPreview.profession?.en}
          </div>
          {pluginPreview.displayDescription?.zh && (
            <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginTop: 4 }}>
              {pluginPreview.displayDescription.zh}
            </div>
          )}
        </div>
      )}

      {/* Plugin JSON 编辑区 */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
          <EditOutlined style={{ color: 'var(--color-text-secondary)' }} />
          <Text strong style={{ fontSize: 13 }}>plugin.json</Text>
        </div>
        <TextArea
          value={pluginJson}
          onChange={(e) => setPluginJson(e.target.value)}
          autoSize={{ minRows: 8, maxRows: 16 }}
          style={{ fontFamily: 'monospace', fontSize: 12 }}
        />
      </div>

      {/* Agent MD 编辑区 */}
      <div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
          <EditOutlined style={{ color: 'var(--color-text-secondary)' }} />
          <Text strong style={{ fontSize: 13 }}>agent.md</Text>
        </div>
        <TextArea
          value={agentMd}
          onChange={(e) => setAgentMd(e.target.value)}
          autoSize={{ minRows: 8, maxRows: 16 }}
          style={{ fontFamily: 'monospace', fontSize: 12 }}
        />
      </div>

      {/* 底部操作按钮 */}
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', paddingTop: 12, borderTop: '1px solid var(--color-border-light)' }}>
        <Button onClick={close}>取消</Button>
        <Button
          type="primary"
          icon={<RocketOutlined />}
          onClick={handleCreate}
          loading={creating}
        >
          创建专家
        </Button>
      </div>
    </Space>
  );
}