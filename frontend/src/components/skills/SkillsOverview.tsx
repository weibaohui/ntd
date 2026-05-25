import { useState, useEffect, useMemo } from 'react';
import { Spin, Row, Col, Card, Statistic, Input, Space, Button, Tooltip, Dropdown, message } from 'antd';
import type { MenuProps } from 'antd';
import {
  ThunderboltOutlined, AppstoreOutlined, BarChartOutlined,
  SearchOutlined, FolderOpenOutlined, FileOutlined,
  DownloadOutlined, ExportOutlined, ImportOutlined,
} from '@ant-design/icons';
import { EXECUTORS } from '../../types';
import type { SkillMeta, ExecutorSkills } from '../../types';
import * as db from '../../utils/database';
import { SkillTree } from './SkillTree';
import { SkillDetailDrawer } from './SkillDetailDrawer';
import { ImportExportModal } from './ImportExportModal';

export function SkillsOverview() {
  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<ExecutorSkills[]>([]);
  const [searchText, setSearchText] = useState('');
  const [showCategory, setShowCategory] = useState(true);
  const [selectedSkill, setSelectedSkill] = useState<SkillMeta | null>(null);
  const [selectedExecutor, setSelectedExecutor] = useState('');
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportMode, setExportMode] = useState<'import' | 'export'>('export');
  const [initialSelectedSkills, setInitialSelectedSkills] = useState<string[] | undefined>(undefined);

  useEffect(() => {
    setLoading(true);
    db.getSkillsList()
      .then(data => {
        setData(data);
        const withSkills = data.find(e => e.skills.length > 0);
        if (withSkills) {
          setSelectedExecutor(withSkills.executor);
        } else if (data.length > 0) {
          setSelectedExecutor(data[0].executor);
        }
      })
      .catch(err => message.error('加载失败: ' + err.message))
      .finally(() => setLoading(false));
  }, []);

  const handleSkillClick = (skill: SkillMeta, executor: string) => {
    setSelectedSkill(skill);
    setSelectedExecutor(executor);
    setDrawerOpen(true);
  };

  const totalSkills = useMemo(() => data.reduce((sum, e) => sum + e.skills.length, 0), [data]);
  const executorsWithSkills = useMemo(() => data.filter(e => e.skills.length > 0).length, [data]);

  const exportMenuItems: MenuProps['items'] = [
    { key: 'export', icon: <ExportOutlined />, label: '导出选中' },
    { key: 'export-all', icon: <ExportOutlined />, label: '导出全部' },
    { type: 'divider' },
    { key: 'import', icon: <ImportOutlined />, label: '导入' },
  ];

  const handleExportMenuClick: MenuProps['onClick'] = ({ key }) => {
    if (key === 'import') {
      setExportMode('import');
      setInitialSelectedSkills(undefined);
    } else {
      setExportMode('export');
      if (key === 'export-all') {
        const executorData = data.find(e => e.executor === selectedExecutor);
        if (executorData) {
          setInitialSelectedSkills(executorData.skills.map(s => s.name));
        }
      } else {
        setInitialSelectedSkills(undefined);
      }
    }
    setExportModalOpen(true);
  };

  const handleImport = (executor: string) => {
    setSelectedExecutor(executor);
    setExportMode('import');
    setInitialSelectedSkills(undefined);
    setExportModalOpen(true);
  };

  const handleExport = (executor: string, selectAll?: boolean) => {
    setSelectedExecutor(executor);
    setExportMode('export');
    if (selectAll) {
      const executorData = data.find(e => e.executor === executor);
      if (executorData) {
        setInitialSelectedSkills(executorData.skills.map(s => s.name));
      }
    } else {
      setInitialSelectedSkills(undefined);
    }
    setExportModalOpen(true);
  };

  if (loading) {
    return <div style={{ textAlign: 'center', padding: 48 }}><Spin size="large" /></div>;
  }

  return (
    <div>
      <Row gutter={16} style={{ marginBottom: 16 }}>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="Skill 总数"
              value={totalSkills}
              prefix={<ThunderboltOutlined style={{ color: '#7C3AED' }} />}
              valueStyle={{ color: '#7C3AED' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="有 Skills 的执行器"
              value={executorsWithSkills}
              suffix={`/ ${data.length}`}
              prefix={<AppstoreOutlined style={{ color: '#10B981' }} />}
              valueStyle={{ color: '#10B981' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={8}>
          <Card size="small">
            <Statistic
              title="执行器总数"
              value={data.length}
              prefix={<BarChartOutlined style={{ color: '#F97316' }} />}
              valueStyle={{ color: '#F97316' }}
            />
          </Card>
        </Col>
      </Row>

      <Card size="small" style={{ marginBottom: 16 }}>
        <Space wrap style={{ width: '100%', justifyContent: 'space-between' }}>
          <Space wrap>
            <Input
              placeholder="搜索 Skills..."
              prefix={<SearchOutlined />}
              value={searchText}
              onChange={e => setSearchText(e.target.value)}
              style={{ width: 200 }}
              allowClear
            />
            <Tooltip title={showCategory ? '显示扁平结构' : '显示目录结构'}>
              <Button
                icon={showCategory ? <FolderOpenOutlined /> : <FileOutlined />}
                onClick={() => setShowCategory(!showCategory)}
              >
                {showCategory ? '目录视图' : '扁平视图'}
              </Button>
            </Tooltip>
          </Space>
          <Dropdown menu={{ items: exportMenuItems, onClick: handleExportMenuClick }} trigger={['click']}>
            <Button type="primary" icon={<DownloadOutlined />}>
              导入/导出
            </Button>
          </Dropdown>
        </Space>
      </Card>

      <SkillTree
        data={data}
        onSkillClick={handleSkillClick}
        onImport={handleImport}
        onExport={handleExport}
        searchText={searchText}
        showCategory={showCategory}
      />

      <SkillDetailDrawer
        skill={selectedSkill}
        executor={selectedExecutor}
        executorLabel={EXECUTORS.find(e => e.value === selectedExecutor)?.label || selectedExecutor}
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
      />

      <ImportExportModal
        open={exportModalOpen}
        mode={exportMode}
        executor={selectedExecutor}
        data={data}
        initialSelectedSkills={initialSelectedSkills}
        onClose={() => {
          setExportModalOpen(false);
          setInitialSelectedSkills(undefined);
        }}
      />
    </div>
  );
}
