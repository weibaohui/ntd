import { useState } from 'react';
import { Card, Badge, Tag, Space, Empty, Button, Tooltip } from 'antd';
import Typography from 'antd/es/typography';
import {
  FolderOutlined, FileTextOutlined,
  UploadOutlined, DownloadOutlined,
  CaretRightOutlined, CaretDownOutlined,
} from '@ant-design/icons';
import { EXECUTOR_COLORS, formatSize, type SkillTreeNode } from './helpers';
import { EXECUTORS } from '../../types';
import type { SkillMeta, ExecutorSkills } from '../../types';

const { Text } = Typography;

interface SkillTreeProps {
  data: ExecutorSkills[];
  onSkillClick: (skill: SkillMeta, executor: string) => void;
  onImport: (executor: string) => void;
  onExport: (executor: string, all?: boolean) => void;
  searchText: string;
  showCategory: boolean;
}

function buildTree(executorData: ExecutorSkills, searchText: string, showCategory: boolean): SkillTreeNode[] {
  const nodes: SkillTreeNode[] = [];
  const lowerSearch = searchText.toLowerCase();

  executorData.skills.forEach(skill => {
    if (lowerSearch && !skill.name.toLowerCase().includes(lowerSearch) &&
        !skill.description?.toLowerCase().includes(lowerSearch)) {
      return;
    }

    if (showCategory && skill.name.includes('/')) {
      const [category, ...rest] = skill.name.split('/');
      const skillName = rest.join('/');

      let categoryNode = nodes.find(n => n.name === category && n.type === 'category');
      if (!categoryNode) {
        categoryNode = {
          key: `${executorData.executor}-${category}`,
          name: category,
          type: 'category',
          executor: executorData.executor,
          color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
          data: null,
          children: [],
          depth: 0,
        };
        nodes.push(categoryNode);
      }

      categoryNode.children!.push({
        key: `${executorData.executor}-${skill.name}`,
        name: skillName,
        type: 'skill',
        executor: executorData.executor,
        color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
        data: skill,
        depth: 1,
      });
    } else {
      nodes.push({
        key: `${executorData.executor}-${skill.name}`,
        name: skill.name,
        type: 'skill',
        executor: executorData.executor,
        color: EXECUTOR_COLORS[executorData.executor] || '#7C3AED',
        data: skill,
        depth: 0,
      });
    }
  });

  return nodes;
}

function SkillTreeNodeItem({ node, expandedKeys, setExpandedKeys, onSkillClick }: {
  node: SkillTreeNode;
  expandedKeys: string[];
  setExpandedKeys: (keys: string[]) => void;
  onSkillClick: (skill: SkillMeta, executor: string) => void;
}) {
  if (node.type === 'category') {
    const isExpanded = expandedKeys.includes(node.key);
    return (
      <div key={node.key}>
        <div
          style={{
            padding: '8px 12px',
            marginBottom: 4,
            borderRadius: 6,
            background: `${node.color}10`,
            borderLeft: `3px solid ${node.color}`,
            cursor: 'pointer',
          }}
          onClick={() => setExpandedKeys(
            isExpanded
              ? expandedKeys.filter(k => k !== node.key)
              : [...expandedKeys, node.key]
          )}
        >
          <Space>
            {isExpanded ? <CaretDownOutlined /> : <CaretRightOutlined />}
            <FolderOutlined style={{ color: node.color }} />
            <Text strong style={{ color: node.color }}>{node.name}</Text>
            <Badge count={node.children?.length} style={{ backgroundColor: node.color }} />
          </Space>
        </div>
        {isExpanded && node.children && (
          <div style={{ marginLeft: 16 }}>
            {node.children.map(child => (
              <SkillTreeNodeItem
                key={child.key}
                node={child}
                expandedKeys={expandedKeys}
                setExpandedKeys={setExpandedKeys}
                onSkillClick={onSkillClick}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      key={node.key}
      style={{
        padding: '8px 12px',
        marginLeft: node.depth * 24,
        marginBottom: 4,
        borderRadius: 6,
        cursor: 'pointer',
        transition: 'all 0.2s',
        border: '1px solid transparent',
      }}
      className="skill-item"
      onClick={() => node.data && onSkillClick(node.data, node.executor)}
      onMouseEnter={e => {
        e.currentTarget.style.background = `${node.color}10`;
        e.currentTarget.style.borderColor = node.color;
      }}
      onMouseLeave={e => {
        e.currentTarget.style.background = 'transparent';
        e.currentTarget.style.borderColor = 'transparent';
      }}
    >
      <Space>
        <FileTextOutlined style={{ color: node.color }} />
        <Text>{node.name}</Text>
        {node.data?.version && <Tag color={node.color} style={{ fontSize: 11 }}>v{node.data.version}</Tag>}
        <Text type="secondary" style={{ fontSize: 12 }}>{formatSize(node.data?.total_size || 0)}</Text>
      </Space>
    </div>
  );
}

export function SkillTree({ data, onSkillClick, onImport, onExport, searchText, showCategory }: SkillTreeProps) {
  const [expandedKeys, setExpandedKeys] = useState<string[]>([]);

  return (
    <div>
      {data.map(executorData => {
        const nodes = buildTree(executorData, searchText, showCategory);
        const executorLabel = EXECUTORS.find(e => e.value === executorData.executor)?.label || executorData.executor;
        // agents 是只读 skill 来源：禁止导入覆盖；其他执行器正常
        const isReadonly = executorData.executor === 'agents';

        return (
          <Card
            key={executorData.executor}
            size="small"
            title={
              <Space>
                <span style={{
                  width: 8, height: 8, borderRadius: '50%',
                  backgroundColor: executorData.skills_dir_exists
                    ? EXECUTOR_COLORS[executorData.executor] || '#7C3AED'
                    : '#d9d9d9',
                }} />
                <Text strong>{executorLabel}</Text>
                {isReadonly && <Tag color="default" style={{ fontSize: 10 }}>只读</Tag>}
                <Badge
                  count={executorData.skills.length}
                  style={{ backgroundColor: executorData.skills.length > 0 ? EXECUTOR_COLORS[executorData.executor] : '#d9d9d9' }}
                />
              </Space>
            }
            extra={
              <Space size="small">
                <Tooltip title={isReadonly ? 'agents 是只读来源，不能导入' : '导入 Skills'}>
                  <Button
                    type="text"
                    size="small"
                    icon={<UploadOutlined />}
                    aria-label="导入 Skills"
                    disabled={isReadonly}
                    onClick={() => onImport(executorData.executor)}
                  />
                </Tooltip>
                <Button
                  type="text"
                  size="small"
                  icon={<DownloadOutlined />}
                  aria-label="导出 Skills"
                  onClick={() => onExport(executorData.executor, false)}
                />
              </Space>
            }
            style={{ marginBottom: 12 }}
          >
            {!executorData.skills_dir_exists ? (
              <Empty description="目录不存在" image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : nodes.length === 0 ? (
              <Empty description={searchText ? '无匹配结果' : '暂无 Skills'} image={Empty.PRESENTED_IMAGE_SIMPLE} />
            ) : (
              nodes.map(node => (
                <SkillTreeNodeItem
                  key={node.key}
                  node={node}
                  expandedKeys={expandedKeys}
                  setExpandedKeys={setExpandedKeys}
                  onSkillClick={onSkillClick}
                />
              ))
            )}
          </Card>
        );
      })}
    </div>
  );
}
