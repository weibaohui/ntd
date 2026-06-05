import { useState, useEffect, useMemo } from 'react';
import { Card, Select, Checkbox, Row, Col, Tag, Space, Alert, Button, Spin, message } from 'antd';
import Typography from 'antd/es/typography';
import { CopyOutlined } from '@ant-design/icons';
import { EXECUTORS, type ExecutorSkills } from '../../types';
import { EXECUTOR_COLORS, formatSize } from './helpers';
import * as db from '../../utils/database';

const { Text, Paragraph } = Typography;

export function SkillSync() {
  const [loading, setLoading] = useState(true);
  const [executors, setExecutors] = useState<ExecutorSkills[]>([]);
  const [selectedExecutor, setSelectedExecutor] = useState<string | null>(null);
  const [selectedSkill, setSelectedSkill] = useState<string | null>(null);
  const [targetExecutors, setTargetExecutors] = useState<string[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [syncResult, setSyncResult] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    db.getSkillsList()
      .then(data => {
        setExecutors(data.filter(e => e.skills_dir_exists));
      })
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const sourceSkills = useMemo(() => {
    if (!selectedExecutor) return [];
    return executors.find(e => e.executor === selectedExecutor)?.skills || [];
  }, [selectedExecutor, executors]);

  const handleSync = async () => {
    if (!selectedExecutor || !selectedSkill || targetExecutors.length === 0) {
      message.warning('请选择源执行器、Skill 和目标执行器');
      return;
    }
    setSyncing(true);
    setSyncResult(null);
    try {
      const result = await db.syncSkill(selectedExecutor, selectedSkill, targetExecutors);
      setSyncResult(result);
      message.success('同步完成');
    } catch (err: any) {
      message.error('同步失败: ' + (err?.message || String(err)));
    } finally {
      setSyncing(false);
    }
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div style={{ maxWidth: 800 }}>
      <Card title="Skill 同步" size="small" style={{ marginBottom: 16 }}>
        <Paragraph type="secondary" style={{ marginBottom: 16 }}>
          将一个执行器下的 Skill 复制到其他执行器。支持批量同步到多个目标。
        </Paragraph>
        <Alert
          type="info"
          showIcon
          style={{ marginBottom: 12 }}
          message="`agents` 是只读 skill 来源（扫描 `~/.agents/skills`），只能作为源，不能作为目标"
        />

        <Space direction="vertical" style={{ width: '100%' }} size="middle">
          <div>
            <Text strong style={{ display: 'block', marginBottom: 8 }}>1. 选择源执行器</Text>
            <Select
              value={selectedExecutor}
              onChange={v => { setSelectedExecutor(v); setSelectedSkill(null); }}
              style={{ width: '100%' }}
              placeholder="选择有 Skills 的执行器"
            >
              {executors.map(e => (
                <Select.Option key={e.executor} value={e.executor}>
                  <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{
                      width: 8, height: 8, borderRadius: '50%',
                      backgroundColor: EXECUTOR_COLORS[e.executor] || '#7C3AED',
                    }} />
                    {e.executor_label}
                    <Tag>{e.skills.length} Skills</Tag>
                  </span>
                </Select.Option>
              ))}
            </Select>
          </div>

          {selectedExecutor && (
            <div>
              <Text strong style={{ display: 'block', marginBottom: 8 }}>2. 选择要同步的 Skill</Text>
              <Select
                value={selectedSkill}
                onChange={setSelectedSkill}
                style={{ width: '100%' }}
                placeholder="选择 Skill"
                showSearch
                optionFilterProp="label"
              >
                {sourceSkills.map(s => (
                  <Select.Option key={s.name} value={s.name} label={s.name}>
                    <span>
                      <Text strong>{s.name}</Text>
                      {s.version && <Tag color="blue" style={{ marginLeft: 8 }}>v{s.version}</Tag>}
                      <Text type="secondary" style={{ marginLeft: 8, fontSize: 11 }}>{formatSize(s.total_size)}</Text>
                    </span>
                  </Select.Option>
                ))}
              </Select>
            </div>
          )}

          {selectedSkill && (
            <div>
              <Text strong style={{ display: 'block', marginBottom: 8 }}>3. 选择目标执行器</Text>
              <Checkbox.Group
                value={targetExecutors}
                onChange={v => setTargetExecutors(v as string[])}
                style={{ width: '100%' }}
              >
                <Row gutter={[8, 8]}>
                  {EXECUTORS.filter(e => e.value !== selectedExecutor).map(exec => {
                    // agents 是只读来源：不能作为同步目标
                    const isReadonly = exec.value === 'agents';
                    const exists = executors.find(ex => ex.executor === exec.value);
                    const alreadyHas = exists?.skills.find(s => s.name === selectedSkill);
                    return (
                      <Col span={12} key={exec.value}>
                        <Checkbox value={exec.value} disabled={isReadonly}>
                          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                            <span style={{
                              width: 6, height: 6, borderRadius: '50%',
                              backgroundColor: exec.color,
                            }} />
                            {exec.label}
                            {isReadonly && <Tag color="default" style={{ fontSize: 10 }}>只读</Tag>}
                            {alreadyHas && <Tag color="orange" style={{ fontSize: 10 }}>已存在</Tag>}
                          </span>
                        </Checkbox>
                      </Col>
                    );
                  })}
                </Row>
              </Checkbox.Group>
            </div>
          )}

          {syncResult && (
            <Alert
              message={syncResult}
              type="success"
              showIcon
            />
          )}

          <div style={{ textAlign: 'right' }}>
            <Button
              type="primary"
              icon={<CopyOutlined />}
              onClick={handleSync}
              loading={syncing}
              disabled={!selectedSkill || targetExecutors.length === 0}
            >
              同步到 {targetExecutors.length} 个执行器
            </Button>
          </div>
        </Space>
      </Card>
    </div>
  );
}
