// 专家管理面板：展示已加载的专家列表，支持搜索、查看详情、重新加载。
//
// 数据来源：后端 /api/experts 系列接口（utils/database/experts.ts 已封装）。
// 布局参考 SkillsPanel，使用 PageCard 包装；详情使用 Drawer 弹出，避免跳转打断列表浏览。

import { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import { Button, Input, Empty, Spin, Drawer, Tag, Tooltip, Typography, App, Modal, Dropdown } from 'antd';
import type { MenuProps } from 'antd';
import {
  TeamOutlined,
  UserOutlined,
  ReloadOutlined,
  SearchOutlined,
  FileTextOutlined,
  ThunderboltOutlined,
  DownloadOutlined,
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
  getExpertAvatarUrl,
} from '@/types/expert';

const { Paragraph, Text } = Typography;

/**
 * 专家卡片：展示头像、名称、类型徽标、职业、描述。
 *
 * 设计取舍：
 * - 头像使用 <img> 而非 antd Avatar，简化 fallback 逻辑：加载失败时切换为类型图标占位。
 * - 类型徽标用 Tag 区分（专家=蓝色，团队=橙色），与 ExpertPicker 中的颜色风格保持一致。
 * - 整卡可点击 + 支持键盘 Enter/Space 触发，符合可访问性要求。
 */
function ExpertCard({ expert, onClick }: {
  expert: ExpertMetadata;
  onClick: (name: string) => void;
}) {
  // 团队类型用橙色徽标，单个专家用蓝色徽标，与 ExpertPicker 色彩风格一致
  const isTeam = expert.expert_type === 'team';
  const displayName = getExpertDisplayName(expert);
  const profession = getExpertProfession(expert);
  const description = getExpertDescription(expert);
  const avatarUrl = getExpertAvatarUrl(expert);
  // 头像加载失败时切换为占位图标，避免出现裂图影响视觉
  const [avatarError, setAvatarError] = useState(false);
  const showAvatar = avatarUrl && !avatarError;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onClick(expert.name)}
      onKeyDown={(e) => {
        // 支持键盘 Enter/Space 触发点击，符合无障碍要求
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick(expert.name);
        }
      }}
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 12,
        padding: 12,
        borderRadius: 10,
        border: '1px solid var(--color-border-secondary)',
        background: 'var(--color-bg-elevated)',
        cursor: 'pointer',
        transition: 'all 0.2s ease',
        height: '100%',
      }}
      onMouseEnter={(e) => {
        // 悬停时边框高亮，提示可点击进入详情
        e.currentTarget.style.borderColor = 'var(--color-primary-hover)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = 'var(--color-border-secondary)';
      }}
    >
      {/* 头像区：40x40 圆形；无头像或加载失败时退化为类型图标占位 */}
      {showAvatar ? (
        <img
          src={avatarUrl}
          alt={displayName}
          onError={() => setAvatarError(true)}
          style={{ width: 40, height: 40, borderRadius: '50%', objectFit: 'cover', flexShrink: 0 }}
        />
      ) : (
        <div style={{
          width: 40,
          height: 40,
          borderRadius: '50%',
          background: isTeam ? 'var(--color-warning-bg-1)' : 'var(--color-info-bg-1)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          flexShrink: 0,
        }}>
          {isTeam ? (
            <TeamOutlined style={{ color: 'var(--color-warning)', fontSize: 18 }} />
          ) : (
            <UserOutlined style={{ color: 'var(--color-info)', fontSize: 18 }} />
          )}
        </div>
      )}

      {/* 文本信息区：名称+徽标 / 职业 / 描述（描述最多 2 行，超出省略） */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
          <Text strong style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {displayName}
          </Text>
          <Tag color={isTeam ? 'orange' : 'blue'} style={{ margin: 0, fontSize: 11 }}>
            {isTeam ? '团队' : '专家'}
          </Tag>
        </div>
        {profession && (
          <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 4 }}>
            {profession}
          </div>
        )}
        {description && (
          <div style={{
            fontSize: 12,
            color: 'var(--color-text-tertiary)',
            display: '-webkit-box',
            WebkitLineClamp: 2,
            WebkitBoxOrient: 'vertical',
            overflow: 'hidden',
            lineHeight: 1.4,
          }}>
            {description}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 渲染 Agent MD 内容区块。
 * 抽出为独立函数以保证 ExpertDetailDrawer 主函数体不超过 30 行（CLAUDE.md 函数长度规范）。
 */
function renderAgentMdSection(agentMd: string) {
  return (
    <div>
      <div style={{ fontWeight: 600, marginBottom: 8, display: 'flex', alignItems: 'center', gap: 6 }}>
        <FileTextOutlined />
        Agent MD
      </div>
      {agentMd ? (
        // 使用 <pre> 保留原始换行与缩进，与 ExecutorsPanel 测试结果展示风格一致
        <pre style={{
          background: 'var(--color-bg-container)',
          color: 'var(--color-text-secondary)',
          padding: 12,
          borderRadius: 6,
          fontSize: 12,
          maxHeight: 320,
          overflow: 'auto',
          whiteSpace: 'pre-wrap',
          margin: 0,
          border: '1px solid var(--color-border-secondary)',
        }}>
          {agentMd}
        </pre>
      ) : (
        <Text type="secondary">暂无 Agent MD 内容</Text>
      )}
    </div>
  );
}

/**
 * 渲染关联技能列表区块。
 * 抽出为独立函数以控制 ExpertDetailDrawer 主函数体长度（CLAUDE.md 函数长度规范）。
 */
function renderSkillsSection(skills: SkillMetadata[]) {
  return (
    <div>
      <div style={{ fontWeight: 600, marginBottom: 8, display: 'flex', alignItems: 'center', gap: 6 }}>
        <ThunderboltOutlined />
        关联技能（{skills.length}）
      </div>
      {skills.length === 0 ? (
        <Text type="secondary">暂无关联技能</Text>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {skills.map(skill => (
            // 优先用中文描述，回退到英文或通用 description 字段
            <div key={skill.skill_name} style={{
              padding: '8px 12px',
              borderRadius: 6,
              background: 'var(--color-bg-container)',
              border: '1px solid var(--color-border-secondary)',
            }}>
              <div style={{ fontWeight: 500, fontSize: 13 }}>
                {skill.yaml_emoji && <span style={{ marginRight: 4 }}>{skill.yaml_emoji}</span>}
                {skill.skill_name}
              </div>
              {(skill.yaml_description_zh || skill.yaml_description || skill.yaml_description_en) && (
                <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 2 }}>
                  {skill.yaml_description_zh || skill.yaml_description || skill.yaml_description_en}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * 专家详情抽屉：打开时按需加载元数据 / Agent MD / 技能列表。
 *
 * 设计取舍：
 * - 抽屉打开时才并发拉取三组数据，避免列表页一次性加载所有专家详情造成性能压力。
 * - Agent MD / Skills 单独 catch 兜底：即便它们加载失败，基本信息仍可展示，提升容错。
 */
function ExpertDetailDrawer({ open, expertName, onClose }: {
  open: boolean;
  expertName: string | null;
  onClose: () => void;
}) {
  const { message } = App.useApp();
  // 详情、agentMd、skills 三组数据用三个 state 持有，便于独立控制 loading 与渲染
  const [expert, setExpert] = useState<ExpertMetadata | null>(null);
  const [agentMd, setAgentMd] = useState<string>('');
  const [skills, setSkills] = useState<SkillMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  // 导出按钮 loading 态：与详情加载区分
  const [exporting, setExporting] = useState(false);

  /** 按 expertName 并发加载详情、Agent MD、技能三组数据 */
  const loadDetail = useCallback(async (name: string) => {
    setLoading(true);
    // 重置上一轮的数据，避免抽屉动画期间短暂闪烁旧内容
    setExpert(null);
    setAgentMd('');
    setSkills([]);
    try {
      // 三组数据并发加载，减少串行等待时间
      const [expertData, mdContent, skillList] = await Promise.all([
        db.getExpertByName(name),
        // Agent MD 加载失败不阻塞其余展示，返回空字符串走"暂无内容"分支
        db.getExpertAgentMd(name).catch(() => ''),
        // 技能加载失败同样不阻塞，返回空数组走"暂无关联技能"分支
        db.getExpertSkills(name).catch(() => [] as SkillMetadata[]),
      ]);
      setExpert(expertData);
      setAgentMd(mdContent);
      setSkills(skillList);
    } catch (err: any) {
      // 主详情加载失败才提示错误（说明后端有问题或专家已被删除）
      message.error('加载专家详情失败: ' + (err?.message || String(err)));
    } finally {
      setLoading(false);
    }
  }, [message]);

  // 抽屉打开时触发详情加载；关闭时不主动清空，避免动画期间出现空态闪烁
  useEffect(() => {
    if (open && expertName) {
      loadDetail(expertName);
    }
  }, [open, expertName, loadDetail]);

  /**
   * 导出专家为 zip 文件。
   * 调用后端导出接口，拿到 Blob 后创建临时 a 标签触发浏览器下载。
   */
  const handleExport = useCallback(async () => {
    if (!expert) return;
    setExporting(true);
    try {
      const blob = await db.exportExpert(expert.name);
      // 创建临时 URL 并触发下载，文件名使用专家名称
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `${expert.name}.zip`;
      document.body.appendChild(a);
      a.click();
      // 下载完成后清理临时 DOM 和 URL，避免内存泄漏
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      message.success('导出成功');
    } catch (err: any) {
      message.error('导出失败: ' + (err?.message || String(err)));
    } finally {
      setExporting(false);
    }
  }, [expert, message]);

  return (
    <Drawer
      title={expert ? getExpertDisplayName(expert) : '专家详情'}
      open={open}
      onClose={onClose}
      width={640}
      styles={{ body: { padding: 16 } }}
      extra={
        expert && (
          <Tooltip title="导出为 zip 包">
            <Button
              type="text"
              icon={<DownloadOutlined />}
              onClick={handleExport}
              loading={exporting}
            >
              导出
            </Button>
          </Tooltip>
        )
      }
    >
      <Spin spinning={loading}>
        {expert && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
            {/* 基本信息区：类型、版本、职业、描述 */}
            <div>
              <div style={{ display: 'flex', gap: 8, marginBottom: 8, flexWrap: 'wrap' }}>
                <Tag color={expert.expert_type === 'team' ? 'orange' : 'blue'}>
                  {expert.expert_type === 'team' ? '团队' : '专家'}
                </Tag>
                <Tag>版本 {expert.version}</Tag>
                {getExpertProfession(expert) && (
                  <Tag color="geekblue">{getExpertProfession(expert)}</Tag>
                )}
              </div>
              {getExpertDescription(expert) && (
                <Paragraph type="secondary" style={{ marginBottom: 0 }}>
                  {getExpertDescription(expert)}
                </Paragraph>
              )}
            </div>
            {/* Agent MD 内容区 */}
            {renderAgentMdSection(agentMd)}
            {/* 关联技能列表区 */}
            {renderSkillsSection(skills)}
          </div>
        )}
      </Spin>
    </Drawer>
  );
}

/**
 * 专家管理面板入口。
 *
 * 功能：
 * - 展示所有已加载的专家列表（卡片网格布局，auto-fill 自适应列数）
 * - 关键字搜索过滤（按名称、职业、描述模糊匹配）
 * - 点击卡片弹出抽屉查看详情（Agent MD、技能列表）
 * - 右上角"重新加载"按钮，触发后端重新扫描 ~/.ntd/experts/ 目录
 */
export function ExpertsPanel() {
  const { message } = App.useApp();
  // 专家列表：进入面板时一次性拉取全量，前端做搜索过滤，避免每次输入都打后端
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  // 重新加载按钮的 loading 态：与初始加载区分，避免整页 Spin 闪烁
  const [reloading, setReloading] = useState(false);
  // 搜索关键字：受控输入，实时过滤
  const [searchText, setSearchText] = useState('');
  // 当前选中的专家名称，用于控制详情抽屉的显隐与内容
  const [selectedExpertName, setSelectedExpertName] = useState<string | null>(null);
  // 导入相关：隐藏的 file input ref
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importing, setImporting] = useState(false);
  // 目录导入弹窗状态
  const [dirImportModalOpen, setDirImportModalOpen] = useState(false);
  const [dirImportPath, setDirImportPath] = useState('');
  // 从 WorkBuddy 导入的 loading 态
  const [workbuddyImporting, setWorkbuddyImporting] = useState(false);

  /** 从后端拉取专家列表并写入 state */
  const loadExperts = useCallback(async () => {
    setLoading(true);
    try {
      const data = await db.getAllExperts();
      setExperts(data);
    } catch (err: any) {
      // 加载失败时给出明确错误提示，便于用户排查（如后端服务未启动）
      message.error('加载专家列表失败: ' + (err?.message || String(err)));
      setExperts([]);
    } finally {
      setLoading(false);
    }
  }, [message]);

  // 进入面板时立即加载一次
  useEffect(() => {
    loadExperts();
  }, [loadExperts]);

  /** 触发后端重新扫描专家定义目录，完成后刷新列表 */
  const handleReload = useCallback(async () => {
    setReloading(true);
    try {
      const result = await db.reloadExperts();
      // 后端可能返回部分加载失败的情况，分别给出不同提示
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

  /**
   * 处理 zip 文件上传导入。
   * 从隐藏的 file input 读取文件，调用后端导入接口，完成后刷新列表。
   */
  const handleFileImport = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    // 重置 input value，允许重复选择同一文件
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

  /**
   * 处理从本地目录导入。
   * 打开弹窗让用户输入目录路径，提交后调用后端导入接口。
   */
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

  /**
   * 从 WorkBuddy 批量导入专家。
   * 调用后端接口扫描 ~/.workbuddy/plugins/marketplaces/experts/plugins/ 目录，
   * 将所有未导入的专家/专家团队批量复制到 ~/.ntd/experts/ 目录。
   */
  const handleWorkbuddyImport = useCallback(async () => {
    setWorkbuddyImporting(true);
    try {
      const result = await db.importFromWorkbuddy();
      // 根据导入结果组合提示信息
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

  // 导入下拉菜单：zip 上传 / 从 WorkBuddy 导入 / 从目录导入
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

  // 按关键字过滤：名称、职业、描述任一命中即保留；空关键字时返回全量
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

  return (
    <PageCard
      icon={<TeamOutlined />}
      title="专家"
      extra={
        <div style={{ display: 'flex', gap: 4 }}>
          {/* 导入下拉按钮：支持 zip 上传和从目录导入两种方式 */}
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
          {/* 重新加载按钮：使用 type="text" 无边框样式，与 LeftRail 按钮风格一致 */}
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
      {/* 顶部搜索栏 + 统计 */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
        <Input
          allowClear
          size="small"
          placeholder="搜索专家名称、职业或描述"
          prefix={<SearchOutlined />}
          value={searchText}
          onChange={e => setSearchText(e.target.value)}
          style={{ width: 280 }}
        />
        <span style={{ color: 'var(--color-text-secondary)', fontSize: 12 }}>
          共 {filteredExperts.length} 个专家
        </span>
      </div>
      {/* 卡片网格区：auto-fill 自适应列数，minmax(260px, 1fr) 保证窄屏单列、宽屏多列 */}
      <Spin spinning={loading}>
        {filteredExperts.length === 0 && !loading ? (
          <Empty description="暂无专家" image={Empty.PRESENTED_IMAGE_SIMPLE} style={{ padding: '48px 0' }} />
        ) : (
          <div style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))',
            gap: 12,
          }}>
            {filteredExperts.map(expert => (
              <ExpertCard key={expert.name} expert={expert} onClick={setSelectedExpertName} />
            ))}
          </div>
        )}
      </Spin>
      {/* 详情抽屉：选中专家后弹出，内部按需加载 Agent MD 与技能列表 */}
      <ExpertDetailDrawer
        open={selectedExpertName !== null}
        expertName={selectedExpertName}
        onClose={() => setSelectedExpertName(null)}
      />
      {/* 隐藏的 zip 上传：由 下拉菜单触发点击 */}
      <input
        ref={fileInputRef}
        type="file"
        accept=".zip"
        style={{ display: 'none' }}
        onChange={handleFileImport}
      />
      {/* 目录导入弹窗：让用户输入本地目录路径，从 WorkBuddy 等位置导入 */}
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
            onChange={e => setDirImportPath(e.target.value)}
          />
        </div>
      </Modal>
    </PageCard>
  );
}
