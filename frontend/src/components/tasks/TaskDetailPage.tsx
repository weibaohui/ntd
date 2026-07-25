// 任务详情页：任务信息 + 工艺要求 + 执行历史。

import { useEffect, useState } from 'react';
import { Card, List, Tag, Button, Typography, Spin, Collapse, Space, message, Descriptions, Modal, Input } from 'antd';
import { ArrowLeftOutlined, ThunderboltOutlined, CaretRightOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import { ProcessExecutionBoard } from '@/components/process/ProcessExecutionBoard';

const { Title, Text } = Typography;

interface TaskDetailProps { taskId: number; onBack: () => void; }
interface StepInfo { id: number; name: string; order_index: number; skill_names: string[]; expected_artifacts: any[]; gate_config: any[]; }
interface ExecInfo { id: number; status: string; started_at?: string; finished_at?: string; total_steps: number; completed_steps: number; failed_steps: number; requirement?: string; }

export function TaskDetailPage({ taskId, onBack }: TaskDetailProps) {
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<any>(null);
  const [activeExec, setActiveExec] = useState<number | null>(null);
  const [triggering, setTriggering] = useState(false);
  const [reqModalOpen, setReqModalOpen] = useState(false);
  const [newRequirement, setNewRequirement] = useState('');

  const load = async () => {
    setLoading(true);
    try { setDetail(await bundledApi.getTaskDetail(taskId)); }
    catch { message.error('加载失败'); }
    finally { setLoading(false); }
  };
  useEffect(() => { load(); }, [taskId]);

  const handleNewExec = async () => {
    if (!newRequirement.trim()) { message.warning('请输入需求'); return; }
    if (!detail) return;
    const wsId = detail.task?.workspace_id || detail.loop?.workspace_id || 1;
    const tmpl = detail.template?.name;
    setTriggering(true);
    try {
      const r = await bundledApi.createTask(newRequirement, wsId, tmpl);
      message.success(`执行 #${r.execution_id} 已创建`);
      setReqModalOpen(false); setNewRequirement(''); load();
    } catch { message.error('创建失败'); }
    finally { setTriggering(false); }
  };

  if (loading) return <Spin style={{ margin: '40px auto', display: 'block' }} />;
  if (!detail) return null;

  const { task, template, steps, executions } = detail;
  const gateLabel = (t: string) => ({ artifact_present: '产物存在', ai_criteria_review: 'AI评审', human_approval: '人工审批', script_check: '脚本校验' }[t] || t);
  const wsId = task?.workspace_id || detail.loop?.workspace_id || 1;
  const lpId = task?.loop_id || detail.loop?.id || 0;

  return (
    <div style={{ padding: 24, height: '100%', overflow: 'auto' }}>
      <Button icon={<ArrowLeftOutlined />} onClick={onBack} style={{ marginBottom: 16 }}>返回任务列表</Button>
      <Card style={{ marginBottom: 16 }}>
        <Descriptions title={<Title level={4} style={{ margin: 0 }}>{task.title}</Title>} size="small" column={2}>
          <Descriptions.Item label="模板">{template?.display_name}</Descriptions.Item>
          <Descriptions.Item label="版本">{template?.version}</Descriptions.Item>
          <Descriptions.Item label="复杂度"><Tag color={template?.complexity === 'light' ? 'green' : template?.complexity === 'standard' ? 'blue' : 'red'}>{template?.complexity}</Tag></Descriptions.Item>
          <Descriptions.Item label="状态"><Tag color={task.status === 'running' ? 'blue' : task.status === 'success' ? 'green' : 'default'}>{task.status}</Tag></Descriptions.Item>
        </Descriptions>
      </Card>

      <Collapse defaultActiveKey={['req']} style={{ marginBottom: 16 }} items={[{
        key: 'req', label: <Text strong>工艺要求（{steps?.length || 0} 步）</Text>,
        children: (steps || []).map((s: StepInfo) => (
          <Card key={s.id} size="small" style={{ marginBottom: 8 }}>
            <Text strong>{s.order_index + 1}. {s.name}</Text>
            <div style={{ marginTop: 4 }}>
              {s.skill_names?.length > 0 && <span>技能：{s.skill_names.map((sk: string) => <Tag key={sk} color="purple">{sk}</Tag>)} </span>}
              {(s.expected_artifacts || []).map((a: any, i: number) => <Tag key={i} color="blue">{a.name} → {a.path || a.locator} ({a.type})</Tag>)}
            </div>
            <div>{(s.gate_config || []).map((g: any, i: number) => <Tag key={i}>{g.name} ({gateLabel(g.type)})</Tag>)}</div>
          </Card>
        )),
      }]} />

      <Title level={4}>执行历史
        <Button icon={<ThunderboltOutlined />} onClick={() => { setNewRequirement(task.description || task.title); setReqModalOpen(true); }} size="small" style={{ marginLeft: 8 }}>新建执行</Button>
      </Title>
      <List dataSource={executions || []} locale={{ emptyText: '暂无执行' }}
        renderItem={(e: ExecInfo) => (
          <List.Item actions={[
            <Button type="link" icon={<CaretRightOutlined />} onClick={() => setActiveExec(activeExec === e.id ? null : e.id)} key="v">{activeExec === e.id ? '收起' : '查看详情'}</Button>,
          ]}>
            <List.Item.Meta
              title={<Space>
                <Tag color={e.status === 'success' ? 'green' : e.status === 'failed' ? 'red' : 'blue'}>{e.status}</Tag>
                <Text>#{e.id} {e.completed_steps}/{e.total_steps} 完成</Text>
                {e.failed_steps > 0 && <Tag color="orange">失败{e.failed_steps}</Tag>}
              </Space>}
              description={e.requirement || <Text type="secondary">{e.started_at?.slice(0,19)?.replace('T',' ')}</Text>}
            />
          </List.Item>
        )} />

      {activeExec && <div style={{ marginTop: 16 }}><ProcessExecutionBoard workspaceId={wsId} loopId={lpId} executionId={activeExec} /></div>}

      <Modal title="输入这次的需求" open={reqModalOpen} onCancel={() => setReqModalOpen(false)} onOk={handleNewExec} confirmLoading={triggering} okText="开始执行">
        <Input.TextArea value={newRequirement} onChange={e => setNewRequirement(e.target.value)} rows={4} />
      </Modal>
    </div>
  );
}
