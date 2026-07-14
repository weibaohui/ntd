// 专家管理面板：Tab 分区展示专家和专家团，支持卡片点击弹出详情 Modal。
//
// 卡片（ExpertCard/TeamCard）与详情弹窗（ExpertDetailModal）已拆分到 ./experts/ 下，
// 本文件只保留列表加载、搜索过滤、导入导出编排与详情 Modal 的状态管理。

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { App, Button, Dropdown, Empty, Input, Modal, Spin, Tabs, Tooltip } from 'antd';
import type { MenuProps } from 'antd';
import {
  TeamOutlined,
  UserOutlined,
  ReloadOutlined,
  SearchOutlined,
  UploadOutlined,
  FolderOpenOutlined,
} from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import * as db from '@/utils/database';
import type { ExpertMetadata, SkillMetadata } from '@/types/expert';
import {
  getExpertDisplayName,
  getExpertDescription,
  getExpertProfession,
} from '@/types/expert';
import { ExpertCreateModal } from './ExpertCreateModal';
import { ExpertCard } from './experts/ExpertCard';
import { TeamCard } from './experts/TeamCard';
import { ExpertDetailModal } from './experts/ExpertDetailModal';

/**
 * 专家管理面板入口
 *
 * 功能：
 * - Tabs 分开展示「专家」和「专家团队」
 * - 关键字搜索过滤（跨两个 Tab）
 * - 点击卡片弹出 Modal 大卡片查看详情
 * - 支持导入（zip、WorkBuddy、目录）和导出功能
 * - 重新加载专家定义目录
 */
export function ExpertsPanel() {
  const { message } = App.useApp();
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [searchText, setSearchText] = useState('');
  // 详情 Modal 相关状态
  const [selectedExpert, setSelectedExpert] = useState<ExpertMetadata | null>(null);
  const [detailOpen, setDetailOpen] = useState(false);
  const [agentMd, setAgentMd] = useState('');
  const [skills, setSkills] = useState<SkillMetadata[]>([]);
  const [exporting, setExporting] = useState(false);
  // 删除相关状态
  const [deleting, setDeleting] = useState(false);
  // 导入相关状态
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importing, setImporting] = useState(false);
  const [dirImportModalOpen, setDirImportModalOpen] = useState(false);
  const [dirImportPath, setDirImportPath] = useState('');
  const [workbuddyImporting, setWorkbuddyImporting] = useState(false);

  // 加载专家列表
  const loadExperts = useCallback(async () => {
    // 任何触发列表刷新的操作（删除/导入/重载）都可能让单专家缓存 stale，
    // 这里统一失效 ExpertBadge 的单专家缓存，避免展示过期数据。
    db.invalidateExpertCache();
    setLoading(true);
    try {
      const data = await db.getAllExperts();
      setExperts(data);
    } catch (err: any) {
      message.error('加载专家列表失败: ' + (err?.message || String(err)));
      setExperts([]);
    } finally {
      setLoading(false);
    }
  }, [message]);

  // 初始加载
  useEffect(() => {
    loadExperts();
  }, [loadExperts]);

  // 重新加载
  const handleReload = useCallback(async () => {
    setReloading(true);
    try {
      const result = await db.reloadExperts();
      if (result.errors.length > 0) {
        message.warning(`加载完成：${result.loaded_count} 个成功，${result.errors.length} 个失败`);
      } else {
        message.success(`已重新加载 ${result.loaded_count} 个专家`);
      }
      await loadExperts();
    } catch (err: any) {
      message.error('重新加载失败: ' + (err?.message || String(err)));
    } finally {
      setReloading(false);
    }
  }, [loadExperts, message]);

  // 打开详情 Modal
  const handleOpenDetail = useCallback(async (expert: ExpertMetadata) => {
    setSelectedExpert(expert);
    setDetailOpen(true);
    setAgentMd('');
    setSkills([]);
    // 异步加载详情数据
    try {
      const [mdContent, skillList] = await Promise.all([
        db.getExpertAgentMd(expert.name).catch(() => ''),
        db.getExpertSkills(expert.name).catch(() => [] as SkillMetadata[]),
      ]);
      setAgentMd(mdContent);
      setSkills(skillList);
    } catch {
      // 加载失败不阻塞展示
    }
  }, []);

  // 关闭详情 Modal
  const handleCloseDetail = useCallback(() => {
    setDetailOpen(false);
    setSelectedExpert(null);
    setAgentMd('');
    setSkills([]);
  }, []);

  // 导出专家
  const handleExport = useCallback(async () => {
    if (!selectedExpert) return;
    setExporting(true);
    try {
      const blob = await db.exportExpert(selectedExpert.name);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${selectedExpert.name}.zip`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('导出成功');
    } catch (err: any) {
      message.error('导出失败: ' + (err?.message || String(err)));
    } finally {
      setExporting(false);
    }
  }, [selectedExpert, message]);

  // 删除专家
  const handleDelete = useCallback(async () => {
    if (!selectedExpert) return;
    setDeleting(true);
    try {
      await db.deleteExpert(selectedExpert.name);
      message.success('删除成功');
      handleCloseDetail();
      await loadExperts();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    } finally {
      setDeleting(false);
    }
  }, [selectedExpert, message, handleCloseDetail, loadExperts]);

  // 文件上传导入
  const handleFileImport = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    setImporting(true);
    try {
      const result = await db.importExpert(file);
      if (result.errors.length > 0) {
        message.warning(`导入完成：${result.errors.length} 条警告`);
      } else {
        message.success('导入成功');
      }
      await loadExperts();
    } catch (err: any) {
      message.error('导入失败: ' + (err?.message || String(err)));
    } finally {
      setImporting(false);
    }
  }, [loadExperts, message]);

  // 目录导入
  const handleDirImport = useCallback(async () => {
    if (!dirImportPath.trim()) {
      message.warning('请输入目录路径');
      return;
    }
    setImporting(true);
    try {
      const result = await db.importExpertFromDirectory(dirImportPath.trim());
      if (result.errors.length > 0) {
        message.warning(`导入完成：${result.errors.length} 条警告`);
      } else {
        message.success('导入成功');
      }
      setDirImportModalOpen(false);
      setDirImportPath('');
      await loadExperts();
    } catch (err: any) {
      message.error('导入失败: ' + (err?.message || String(err)));
    } finally {
      setImporting(false);
    }
  }, [dirImportPath, loadExperts, message]);

  // WorkBuddy 批量导入
  const handleWorkbuddyImport = useCallback(async () => {
    setWorkbuddyImporting(true);
    try {
      const result = await db.importFromWorkbuddy();
      const parts: string[] = [];
      if (result.imported_count > 0) parts.push(`新增 ${result.imported_count} 个`);
      if (result.skipped_count > 0) parts.push(`跳过 ${result.skipped_count} 个（已存在）`);
      if (result.errors.length > 0) parts.push(`${result.errors.length} 个失败`);

      if (parts.length === 0) {
        message.info('WorkBuddy 目录中未找到可导入的专家');
      } else if (result.errors.length > 0) {
        message.warning(`WorkBuddy 导入：${parts.join('，')}`);
      } else {
        message.success(`WorkBuddy 导入：${parts.join('，')}`);
      }
      await loadExperts();
    } catch (err: any) {
      message.error('WorkBuddy 导入失败: ' + (err?.message || String(err)));
    } finally {
      setWorkbuddyImporting(false);
    }
  }, [loadExperts, message]);

  // 搜索过滤
  const filteredExperts = useMemo(() => {
    if (!searchText.trim()) return experts;
    const keyword = searchText.trim().toLowerCase();
    return experts.filter(expert => {
      const name = getExpertDisplayName(expert).toLowerCase();
      const profession = getExpertProfession(expert).toLowerCase();
      const description = getExpertDescription(expert).toLowerCase();
      return name.includes(keyword) || profession.includes(keyword) || description.includes(keyword);
    });
  }, [experts, searchText]);

  // 按类型分组并按名称排序（稳定排序，避免刷新时顺序跳动）
  const individualExperts = useMemo(() => {
    return filteredExperts
      .filter(e => e.expert_type === 'agent')
      .sort((a, b) => {
        const nameA = getExpertDisplayName(a).toLowerCase();
        const nameB = getExpertDisplayName(b).toLowerCase();
        return nameA.localeCompare(nameB, 'zh-CN');
      });
  }, [filteredExperts]);
  const teamExperts = useMemo(() => {
    return filteredExperts
      .filter(e => e.expert_type === 'team')
      .sort((a, b) => {
        const nameA = getExpertDisplayName(a).toLowerCase();
        const nameB = getExpertDisplayName(b).toLowerCase();
        return nameA.localeCompare(nameB, 'zh-CN');
      });
  }, [filteredExperts]);

  // 导入下拉菜单
  const importMenuItems: MenuProps['items'] = [
    {
      key: 'zip',
      label: '上传 zip 包',
      icon: <UploadOutlined />,
      onClick: () => fileInputRef.current?.click(),
    },
    {
      key: 'workbuddy',
      label: '从 WorkBuddy 导入',
      icon: <TeamOutlined />,
      onClick: handleWorkbuddyImport,
      disabled: workbuddyImporting,
    },
    {
      key: 'dir',
      label: '从本地目录导入',
      icon: <FolderOpenOutlined />,
      onClick: () => setDirImportModalOpen(true),
    },
  ];

  return (
    <PageCard
      icon={<TeamOutlined />}
      title="专家"
      extra={
        <div style={{ display: 'flex', gap: 4 }}>
          <Dropdown menu={{ items: importMenuItems }} placement="bottomRight">
            <Tooltip title="导入专家">
              <Button
                type="text"
                icon={<UploadOutlined />}
                loading={importing}
              >
                导入
              </Button>
            </Tooltip>
          </Dropdown>
          <Tooltip title="重新扫描 ~/.ntd/experts/ 目录">
            <Button
              type="text"
              icon={<ReloadOutlined spin={reloading} />}
              onClick={handleReload}
              disabled={reloading}
            >
              重新加载
            </Button>
          </Tooltip>
        </div>
      }
    >
      {/* 搜索栏 + AI 创建专家按钮 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 16, flexWrap: 'wrap' }}>
        <Input
          allowClear
          size="small"
          placeholder="搜索专家名称、职业或描述"
          prefix={<SearchOutlined />}
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          style={{ width: 280 }}
        />
        <ExpertCreateModal
          onCreated={loadExperts}
        />
        <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
          共 {filteredExperts.length} 个（专家 {individualExperts.length} / 团队 {teamExperts.length}）
        </span>
      </div>

      {/* Tabs 分区 */}
      <Tabs
        defaultActiveKey="experts"
        items={[
          {
            key: 'experts',
            label: (
              <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <UserOutlined />
                专家 ({individualExperts.length})
              </span>
            ),
            children: (
              <Spin spinning={loading}>
                {individualExperts.length === 0 && !loading ? (
                  <Empty
                    description="暂无专家"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    style={{ padding: '48px 0' }}
                  />
                ) : (
                  <div style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
                    gap: 14,
                  }}>
                    {individualExperts.map((expert) => (
                      <ExpertCard
                        key={expert.name}
                        expert={expert}
                        onClick={handleOpenDetail}
                      />
                    ))}
                  </div>
                )}
              </Spin>
            ),
          },
          {
            key: 'teams',
            label: (
              <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <TeamOutlined />
                专家团队 ({teamExperts.length})
              </span>
            ),
            children: (
              <Spin spinning={loading}>
                {teamExperts.length === 0 && !loading ? (
                  <Empty
                    description="暂无专家团队"
                    image={Empty.PRESENTED_IMAGE_SIMPLE}
                    style={{ padding: '48px 0' }}
                  />
                ) : (
                  <div style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
                    gap: 14,
                  }}>
                    {teamExperts.map((expert) => (
                      <TeamCard
                        key={expert.name}
                        expert={expert}
                        onClick={handleOpenDetail}
                      />
                    ))}
                  </div>
                )}
              </Spin>
            ),
          },
        ]}
      />

      {/* 详情 Modal */}
      <ExpertDetailModal
        open={detailOpen}
        expert={selectedExpert}
        agentMd={agentMd}
        skills={skills}
        onClose={handleCloseDetail}
        onExport={handleExport}
        exporting={exporting}
        onDelete={handleDelete}
        deleting={deleting}
      />

      {/* 隐藏的文件上传 */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".zip"
        style={{ display: 'none' }}
        onChange={handleFileImport}
      />

      {/* 目录导入弹窗 */}
      <Modal
        title="从本地目录导入专家"
        open={dirImportModalOpen}
        onOk={handleDirImport}
        onCancel={() => { setDirImportModalOpen(false); setDirImportPath(''); }}
        okText="导入"
        cancelText="取消"
        confirmLoading={importing}
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          <div style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>
            请输入专家定义目录的绝对路径，例如 WorkBuddy 插件目录。
            <br />
            目录下需包含 <code>.codebuddy-plugin/plugin.json</code> 文件。
          </div>
          <Input
            placeholder="~/.workbuddy/plugins/marketplaces/experts/plugins/senior-developer"
            value={dirImportPath}
            onChange={(e) => setDirImportPath(e.target.value)}
          />
        </div>
      </Modal>
    </PageCard>
  );
}
