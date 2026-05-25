import { useState, useEffect } from 'react';
import { Drawer, Spin, Card, Row, Col, Tag, Typography, Space, Empty, Tooltip } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import * as db from '../../utils/database';
import type { SessionDetail } from '../../utils/database';
import { sourceTag, formatBytes, formatTokens, formatTime, shortId } from './helpers';

const { Text, Paragraph } = Typography;

export function SessionDetailDrawer({
  sessionId,
  open,
  onClose,
}: {
  sessionId: string | null;
  open: boolean;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (open && sessionId) {
      setLoading(true);
      db.getSessionDetail(sessionId)
        .then(setDetail)
        .catch(() => setDetail(null))
        .finally(() => setLoading(false));
    } else {
      setDetail(null);
    }
  }, [open, sessionId]);

  return (
    <Drawer
      title={detail ? (
        <Space>
          {sourceTag(detail.info.source)}
          <span>Session {shortId(detail.info.session_id)}</span>
        </Space>
      ) : 'Session 详情'}
      open={open}
      onClose={onClose}
      width={680}
      styles={{ body: { padding: '16px 24px' } }}
    >
      <Spin spinning={loading}>
        {detail ? (
          <>
            <Card size="small" title="基本信息" style={{ marginBottom: 16 }}>
              <Row gutter={[16, 8]}>
                <Col span={12}><Text type="secondary">工具：</Text>{sourceTag(detail.info.source)}</Col>
                <Col span={12}><Text type="secondary">状态：</Text>
                  <Tag color={detail.info.status === 'active' ? 'green' : 'default'}>
                    {detail.info.status === 'active' ? '活跃' : '已完成'}
                  </Tag>
                </Col>
                <Col span={12}><Text type="secondary">项目：</Text><Text>{detail.info.project_path}</Text></Col>
                <Col span={12}><Text type="secondary">执行器：</Text><Text>{detail.info.executor}</Text></Col>
                <Col span={12}><Text type="secondary">模型：</Text><Text code>{detail.info.model}</Text></Col>
                <Col span={12}><Text type="secondary">Git 分支：</Text><Text code>{detail.info.git_branch || '-'}</Text></Col>
                <Col span={12}><Text type="secondary">版本：</Text><Text code>{detail.info.version || '-'}</Text></Col>
                <Col span={12}><Text type="secondary">消息数：</Text><Text>{detail.info.message_count}</Text></Col>
                <Col span={12}>
                  <Text type="secondary">文件大小：</Text>
                  <Text>{formatBytes(detail.info.file_size)}</Text>
                </Col>
                <Col span={12}>
                  <Text type="secondary">Token：</Text>
                  <Tooltip title={`输入: ${formatTokens(detail.info.total_input_tokens)} / 输出: ${formatTokens(detail.info.total_output_tokens)}`}>
                    <Text>{formatTokens(detail.info.total_input_tokens + detail.info.total_output_tokens)}</Text>
                  </Tooltip>
                </Col>
                <Col span={12}><Text type="secondary">子代理：</Text><Text>{detail.info.subagent_count}</Text></Col>
                <Col span={24}>
                  <Text type="secondary">首条 Prompt：</Text>
                  <Paragraph
                    ellipsis={{ rows: 3, expandable: true, symbol: '展开' }}
                    style={{ marginTop: 4, marginBottom: 0 }}
                  >
                    {detail.info.first_prompt || '-'}
                  </Paragraph>
                </Col>
              </Row>
            </Card>

            {detail.subagents.length > 0 && (
              <Card size="small" title={`子代理 (${detail.subagents.length})`} style={{ marginBottom: 16 }}>
                {detail.subagents.map((sa, i) => (
                  <div key={i} style={{ padding: '6px 0', borderBottom: i < detail.subagents.length - 1 ? '1px solid var(--color-border-light)' : 'none' }}>
                    <Space>
                      <Tag color="purple">{sa.agent_type}</Tag>
                      <Text>{sa.description}</Text>
                    </Space>
                  </div>
                ))}
              </Card>
            )}

            <Card size="small" title={`对话记录 (${detail.messages.length})`}>
              <div style={{ maxHeight: 500, overflowY: 'auto' }}>
                {detail.messages.length === 0 ? (
                  <Empty description="无对话记录" />
                ) : (
                  detail.messages.map((msg, i) => (
                    <div
                      key={i}
                      style={{
                        marginBottom: 12,
                        padding: '8px 12px',
                        borderRadius: 8,
                        background: msg.role === 'user'
                          ? 'var(--color-bg-elevated)'
                          : 'rgba(22, 119, 255, 0.06)',
                        borderLeft: msg.role === 'user'
                          ? '3px solid var(--color-border)'
                          : '3px solid #1677ff',
                      }}
                    >
                      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                        <Space size={4}>
                          <RobotOutlined style={{ color: msg.role === 'user' ? undefined : '#1677ff' }} />
                          <Text strong style={{ fontSize: 12 }}>
                            {msg.role === 'user' ? '用户' : '助手'}
                          </Text>
                          {msg.model && <Tag color="blue" style={{ fontSize: 10, lineHeight: '16px', padding: '0 4px' }}>{msg.model}</Tag>}
                        </Space>
                        <Space size={8}>
                          {msg.input_tokens != null && (
                            <Text type="secondary" style={{ fontSize: 11 }}>
                              ↑{formatTokens(msg.input_tokens)}
                            </Text>
                          )}
                          {msg.output_tokens != null && (
                            <Text type="secondary" style={{ fontSize: 11 }}>
                              ↓{formatTokens(msg.output_tokens)}
                            </Text>
                          )}
                          <Text type="secondary" style={{ fontSize: 11 }}>{formatTime(msg.timestamp)}</Text>
                        </Space>
                      </div>
                      <Paragraph
                        ellipsis={{ rows: 4, expandable: true, symbol: '展开' }}
                        style={{ margin: 0, fontSize: 13, whiteSpace: 'pre-wrap' }}
                      >
                        {msg.content_preview || '(无内容)'}
                      </Paragraph>
                    </div>
                  ))
                )}
              </div>
            </Card>
          </>
        ) : (
          <Empty description="选择一个 Session 查看详情" />
        )}
      </Spin>
    </Drawer>
  );
}
