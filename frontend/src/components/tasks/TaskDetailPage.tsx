// 任务详情页：工艺要求 + 执行历史。

import { useEffect, useState } from 'react';
import { Card, List, Tag, Button, Typography, Spin, Collapse, Space, message, Descriptions } from 'antd';
import { ArrowLeftOutlined, ThunderboltOutlined, CaretRightOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';

const { Title, Text } = Typography;

interface TaskDetailProps {
  taskId: number;
  onBack: () => void;
}

interface StepInfo {
  id: number; name: string; order_index: number;
  skill_names: string[]; expected_artifacts: any[]; gate_config: any[];
}

interface ExecInfo {
  id: number; status: string; started_at?: string; finished_at?: string;
  total_steps: number; completed_steps: number; failed_steps: number;
}

export function TaskDetailPage({ taskId, onBack }: TaskDetailProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<{
    loop: { id: number; name: string; description: string; status: string; workspace_id: number };
    template: { name: string; display_name: string; complexity: string; version: string };
    steps: StepInfo[];
    executions: ExecInfo[];
  } | null>(null);
  const [activeExec, setActiveExec] = useState<number | null>(null);
  const [triggering, setTriggering] = useState(false);

  const load = async () => {
    setLoading(true);
    try { setDetail(await bundledApi.getTaskDetail(taskId)); }
    catch { message.error('加载任务详情失败'); }
    finally { setLoading(false); }
  };

  useEffect(() => { load(); }, [taskId]);

  const handleTrigger = async () => {
    setTriggering(true);
    try {
      await bundledApi.triggerLoopExecution(taskId);
      message.success('已创建新执行');
      load();
    } catch { message.error('创建执行失败'); }
    finally { setTriggering(false); }
  };

  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!detail) return null;

  const { loop, template, steps, executions } = detail;
  const gateTypeLabel = (t: string) => ({ artifact_present: '产物存在', ai_criteria_review: 'AI评审', human_approval: '人工审批', script_check: '脚本校验' }[t] || t);

  return (
    <div style={{ padding: 24, maxWidth: 900, margin: '0 auto' }}>
      <Button icon={<ArrowLeftOutlined />} onClick={onBack} style={{ marginBottom: 16 }}>返回任务列表</Button>
      <Card style={{ marginBottom: 16 }}>
        <Descriptions title={<Title level={4} style={{ margin: 0 }}>{loop.description || template.display_name}</Title>} size="small" column={2}>
          <Descriptions.Item label="模板">{template.display_name}</Descriptions.Item>
          <Descriptions.Item label="版本">{template.version}</Descriptions.Item>
          <Descriptions.Item label="复杂度"><Tag color={template.complexity === 'light' ? 'green' : template.complexity === 'standard' ? 'blue' : 'red'}>{template.complexity}</Tag></Descriptions.Item>
          <Descriptions.Item label="状态"><Tag>{loop.status}</Tag></Descriptions.Item>
        </Descriptions>
      </Card>

      <Collapse
        defaultActiveKey={['requirements']}
        style={{ marginBottom: 16 }}
        items={[{
          key: 'requirements',
          label: <Text strong>工艺要求（{steps.length} 个步骤）</Text>,
          children: (
            <div>
              {steps.map((s) => (
                <Card key={s.id} size="small" style={{ marginBottom: 8 }}>
                  <Text strong>{s.order_index + 1}. {s.name}</Text>
                  <div style={{ marginTop: 4 }}>
                    {s.skill_names.length > 0 && (
                      <span>技能：{s.skill_names.map((sk: string) => <Tag key={sk} color="purple">{sk}</Tag>)} </span>
                    )}
                    {Array.isArray(s.expected_artifacts) && (s.expected_artifacts as any[]).map((a: any, i: number) => (
                      <Tag key={i} color="blue">{a.name} → {a.path || a.locator} ({a.type})</Tag>
                    ))}
                  </div>
                  <div style={{ marginTop: 4 }}>
                    {Array.isArray(s.gate_config) && (s.gate_config as any[]).map((g: any, i: number) => (
                      <Tag key={i}>{g.name} ({gateTypeLabel(g.type)})</Tag>
                    ))}
                  </div>
                </Card>
              ))}
            </div>
          ),
        }]}
      />

      <Title level={4}>执行历史 <Button icon={<ThunderboltOutlined />} loading={triggering} onClick={handleTrigger} size="small" style={{ marginLeft: 8 }}>新建执行</Button></Title>
      <List
        dataSource={executions}
        locale={{ emptyText: '暂无执行记录。点击「新建执行」开始。' }}
        renderItem={e => (
          <List.Item
            actions={[
              <Button type="link" icon={<CaretRightOutlined />} onClick={() => setActiveExec(activeExec === e.id ? null : e.id)} key="view">
                {activeExec === e.id ? '收起' : '查看详情'}
              </Button>,
            ]}
          >
            <List.Item.Meta
              title={
                <Space>
                  <Tag color={e.status === 'success' ? 'green' : e.status === 'failed' ? 'red' : e.status === 'running' ? 'blue' : 'default'}>
                    {e.status}
                  </Tag>
                  <Text>执行 #{e.id}</Text>
                  <Text type="secondary">{e.completed_steps}/{e.total_steps} 完成</Text>
                  {e.failed_steps > 0 && <Tag color="orange">失败 {e.failed_steps}</Tag>}
                </Space>
              }
              description={<Text type="secondary">{e.started_at?.slice(0, 19)?.replace('T', ' ')}</Text>}
            />
          </List.Item>
        )}
      />

      {activeExec && (
        <div style={{ marginTop: 16 }}>
          <ProcessExecutionBoard
            workspaceId={loop.workspace_id || 1}
            loopId={loop.id}
            executionId={activeExec}
          />
        </div>
      )}
    </div>
  );
}
