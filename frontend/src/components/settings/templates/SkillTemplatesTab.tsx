// Skill 模板 Tab
// 展示从远程仓库同步的 skills 列表
// 数据从 bundled/skills 目录扫描加载

import { useState, useEffect, useCallback } from 'react';
import {
  Alert,
  App,
  Button,
  Empty,
  Space,
  Spin,
  Table,
  Tag,
  Typography,
} from 'antd';
import {
  ReloadOutlined,
} from '@ant-design/icons';
import { bundledApi, type BundledSkillMeta } from '@/api/bundled';
import { formatSize } from '@/components/skills/helpers';
import { ShareToRepoButton } from '@/components/settings/contribute/ShareToRepoButton';
import { buildSkillContributePrompt } from '@/components/settings/contribute/contributePrompts';

const { Text } = Typography;

/**
 * Skill 模板 Tab
 *
 * 以表格形式展示远程仓库中的技能列表：
 * - 刷新列表（重新扫描 bundled/skills 目录）
 * - 查看技能名称、来源、描述、版本等信息
 */
export function SkillTemplatesTab({ refreshTick }: { refreshTick?: number }) {
  const { message } = App.useApp();
  const [skills, setSkills] = useState<BundledSkillMeta[]>([]);
  const [loading, setLoading] = useState(false);

  /**
   * 加载技能列表
   */
  const loadSkills = useCallback(async () => {
    setLoading(true);
    try {
      // 强制分页：page=1, page_size=200 取接近全量的首页切片，
      // 模板配置场景需要一次性看到所有技能
      const res = await bundledApi.getSkills({ page: 1, page_size: 200 });
      setSkills(res.skills);
    } catch (e: any) {
      message.error('加载技能列表失败: ' + (e?.message || e));
    } finally {
      setLoading(false);
    }
  }, [message]);

  // mount 时加载一次；父组件同步成功后会递增 refreshTick，这里据此重拉，保证列表不陈旧。
  useEffect(() => {
    loadSkills();
  }, [loadSkills, refreshTick]);

  const columns = [
    // 操作列置于第一列：与专家/事项/工艺各 Tab 保持一致，操作入口前置便于定位
    {
      title: '操作',
      key: 'actions',
      width: 100,
      // 技能分享不区分来源（用户明确要求全量可分享）：所有行都渲染分享入口。
      render: (_: unknown, record: BundledSkillMeta) => (
        <ShareToRepoButton
          actionType="skill_contribute"
          actionKey={record.name}
          params={{
            resource_name: record.short_name,
            version: record.version || '',
            // bundled 技能目录天然是 ~ 相对路径（~/.ntd/bundled/skills/{source}/{skill}），
            // 无需 toHomePath 转换；远端路径保持来源集合结构 skills/{source}/{skill}/
            resource_dir: `~/.ntd/bundled/skills/${record.name}`,
            remote_path: `skills/${record.name}/`,
          }}
          buildPrompt={buildSkillContributePrompt}
          panelTitle={`分享技能 ${record.short_name}`}
          panelDescription="AI 将读取本机 PAT，把该技能目录提交为 PR 到官方仓库（可编辑下方 Prompt）"
          size="small"
        />
      ),
    },
    {
      title: '名称',
      dataIndex: 'name',
      key: 'name',
      render: (name: string, record: BundledSkillMeta) => (
        <Space direction="vertical" size={0}>
          <Text strong>{record.short_name}</Text>
          <Text type="secondary" style={{ fontSize: 12 }}>{name}</Text>
        </Space>
      ),
    },
    {
      title: '来源',
      dataIndex: 'source',
      key: 'source',
      render: (source: string) => <Tag color="blue">{source}</Tag>,
    },
    {
      title: '描述',
      dataIndex: 'description',
      key: 'description',
      ellipsis: true,
      render: (desc: string, record: BundledSkillMeta) =>
        record.description_zh || desc || '-',
    },
    {
      title: '版本',
      dataIndex: 'version',
      key: 'version',
      render: (v: string) => v || '-',
    },
    {
      title: '文件数',
      dataIndex: 'file_count',
      key: 'file_count',
      width: 80,
    },
    {
      title: '大小',
      dataIndex: 'total_size',
      key: 'total_size',
      width: 100,
      render: (size: number) => formatSize(size),
    },
  ];

  return (
    <div className="skill-templates-tab">
      {/* 技能来源说明：技能全部来自远程仓库同步，同步后本地副本与远端一致；
          分享不区分来源（全量可分享），提交的是当前同步到本地的技能目录。 */}
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="技能列表来自远程仓库同步，每次同步会更新本地副本；所有技能均可分享到官方仓库（不区分来源）。"
      />
      <Space style={{ marginBottom: 16 }}>
        <Button
          icon={<ReloadOutlined />}
          loading={loading}
          onClick={loadSkills}
        >
          刷新列表
        </Button>
        <Text type="secondary">
          共 {skills.length} 个技能
        </Text>
      </Space>

      <Spin spinning={loading}>
        <Table
          dataSource={skills}
          columns={columns}
          rowKey="name"
          size="small"
          // 移动端适配：6 列在窄屏会挤压错位，启用横向滚动（与「事项模板」表一致），
          // 列宽按内容撑开，超出容器可横滑而不压缩。
          scroll={{ x: 'max-content' }}
          pagination={{ pageSize: 20 }}
          locale={{
            emptyText: (
              <Empty description="暂无技能模板，请先同步远程仓库">
                <Text type="secondary">
                  远程仓库的 skills/ 目录将同步到本地
                </Text>
              </Empty>
            ),
          }}
        />
      </Spin>
    </div>
  );
}
