// 工艺执行看板：按阶段分组展示环节的 skill / 产物 / 门禁 / 返工。
// 从审计 API GET .../audit 一次拉取全量数据。

import { useEffect, useState } from 'react';
import {
  Card,
  Collapse,
  Tag,
  Spin,
  Empty,
  Button,
  Space,
  Typography,
  message,
  Progress,
} from 'antd';
import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  ClockCircleOutlined,
  ReloadOutlined,
  FileTextOutlined,
  AuditOutlined,
  ToolOutlined,
} from '@ant-design/icons';
import bundledApi, { type ProcessAuditDto, type PhaseAuditDto, type StepAuditDto, type GateDto } from '@/api/bundled';

const { Text, Title } = Typography;

interface ProcessExecutionBoardProps {
  workspaceId: number;
  loopId: number;
  executionId: number;
  onBack?: () => void;
}

export function ProcessExecutionBoard({ workspaceId, loopId, executionId, onBack }: ProcessExecutionBoardProps) {
  const [audit, setAudit] = useState<ProcessAuditDto | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async () => {
    setLoading(true);
    try {
      const data = await bundledApi.getProcessAudit(workspaceId, loopId, executionId);
      setAudit(data);
    } catch {
      message.error('加载工艺审计数据失败');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, [executionId]);

  const handleApprove = async (gate: GateDto, stepExecId: number) => {
    const approved = !(gate.status === 'passed');
    try {
      await bundledApi.approveGate(workspaceId, loopId, executionId, stepExecId, gate.id, approved);
      message.success(approved ? '已通过' : '已拒绝');
      load();
    } catch {
      message.error('审批操作失败');
    }
  };

  if (loading) return <Spin style={{ display: 'block', margin: '40px auto' }} />;
  if (!audit) return <Empty description="暂无审计数据" />;

  const { loop_execution: le, phases } = audit;
  const completedSteps = phases.reduce((s: number, p: PhaseAuditDto) =>
    s + p.steps.filter((st: StepAuditDto) => st.execution?.status === 'success').length, 0);

  const statusColor = (s: string) => {
    switch (s) {
      case 'success': case 'passed': return 'green';
      case 'failed': return 'red';
      case 'running': case 'pending': case 'pending_approval': return 'blue';
      default: return 'default';
    }
  };

  const statusIcon = (s: string) => {
    switch (s) {
      case 'success': case 'passed': return <CheckCircleOutlined />;
      case 'failed': return <CloseCircleOutlined />;
      default: return <ClockCircleOutlined />;
    }
  };

  return (
    <div style={{ padding: 24 }}>
      <div style={{ marginBottom: 16 }}>
        {onBack && (
          <Button onClick={onBack} style={{ marginRight: 16 }}>← 返回</Button>
        )}
        <Button icon={<ReloadOutlined />} onClick={load}>刷新</Button>
      </div>

      <Card style={{ marginBottom: 16 }}>
        <Title level={4}>Loop #{le.loop_id}「{le.status}」执行看板</Title>
        <Space direction="vertical" style={{ width: '100%' }}>
          <Text>
            进度：{completedSteps}/{le.total_steps} 环节完成 |
            成功：{le.completed_steps} | 失败：{le.failed_steps} |
            状态：<Tag color={statusColor(le.status)}>{le.status}</Tag>
          </Text>
          <Progress
            percent={le.total_steps > 0 ? Math.round((completedSteps / le.total_steps) * 100) : 0}
            size="small"
          />
        </Space>
      </Card>

      <Collapse
        defaultActiveKey={phases.map((_, i: number) => String(i))}
        items={phases.map((phase: PhaseAuditDto, pi: number) => ({
          key: String(pi),
          label: (
            <Space>
              {statusIcon(phase.execution?.status || 'pending')}
              <Text strong>{phase.phase_name}</Text>
              <Tag color={statusColor(phase.execution?.status || 'pending')}>
                {phase.execution?.status || 'pending'}
              </Tag>
            </Space>
          ),
          children: (
            <div>
              {phase.steps.map((step: StepAuditDto) => (
                <Card
                  key={step.step_id}
                  size="small"
                  style={{ marginBottom: 8 }}
                  title={
                    <Space>
                      {statusIcon(step.execution?.status || 'pending')}
                      <Text>{step.step_name}</Text>
                      <Tag color={statusColor(step.execution?.status || 'pending')}>
                        {step.execution?.status || '未执行'}
                      </Tag>
                      {step.execution && step.execution.rework_count > 0 && (
                        <Tag color="orange">返工 x{step.execution.rework_count}</Tag>
                      )}
                    </Space>
                  }
                >
                  {/* Skill 列表 */}
                  {step.skill_names.length > 0 && (
                    <div style={{ marginBottom: 8 }}>
                      <ToolOutlined style={{ marginRight: 4 }} />
                      <Text type="secondary">技能：</Text>
                      {step.skill_names.map((s: string) => (
                        <Tag key={s} color="purple">{s}</Tag>
                      ))}
                    </div>
                  )}

                  {/* 产物列表 */}
                  {step.artifacts.length > 0 && (
                    <div style={{ marginBottom: 8 }}>
                      <FileTextOutlined style={{ marginRight: 4 }} />
                      <Text type="secondary">产物：</Text>
                      {step.artifacts.map((a) => (
                        <Tag key={a.id} color="blue">{a.name} → {a.locator}</Tag>
                      ))}
                    </div>
                  )}

                  {/* 门禁列表 */}
                  {step.gates.length > 0 && (
                    <div>
                      <AuditOutlined style={{ marginRight: 4 }} />
                      <Text type="secondary">门禁：</Text>
                      {step.gates.map((g: GateDto) => (
                        <Tag key={g.id} color={statusColor(g.status)}>
                          {g.gate_name} → {g.status}
                          {g.gate_type === 'human_approval' && g.status === 'pending' && step.execution?.status === 'pending_approval' && (
                            <span style={{ marginLeft: 4 }}>
                              <Button
                                size="small"
                                type="primary"
                                onClick={() => handleApprove(g, step.execution!.step_execution_id)}
                              >
                                通过
                              </Button>
                              <Button
                                size="small"
                                danger
                                style={{ marginLeft: 4 }}
                                onClick={() => handleApprove(g, step.execution!.step_execution_id)}
                              >
                                拒绝
                              </Button>
                            </span>
                          )}
                          {g.result && (
                            <Text type="secondary" style={{ marginLeft: 4 }}>({g.result.slice(0, 60)})</Text>
                          )}
                        </Tag>
                      ))}
                    </div>
                  )}
                </Card>
              ))}
            </div>
          ),
        }))}
      />
    </div>
  );
}
