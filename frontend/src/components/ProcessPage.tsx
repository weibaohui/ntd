// 工艺模板库页面：展示已同步的工艺模板列表，支持查看详情与安装为 Loop。

import { useEffect, useState } from 'react';
import {
  Card, Col, Row, Typography, Tag, Button, Empty, Spin,
  Modal, Select, message, Descriptions, Input, Alert,
  Space, Tabs, Table, Segmented,
} from 'antd';
import { BuildOutlined, ReloadOutlined, EyeOutlined, DownloadOutlined, SearchOutlined, ApartmentOutlined, CodeOutlined, PlusOutlined, EditOutlined, CopyOutlined } from '@ant-design/icons';
import { PageCard } from '@/components/common/PageCard';
import bundledApi, { type ProcessTemplate, type ProcessTemplateDetail, type ProcessLoopItem } from '@/api/bundled';
import { adaptProcessDefinition } from '@/components/process/processFlowAdapter';
import { ProcessFlowGraph } from '@/components/process/ProcessFlowGraph';
// M3：真实编辑器组件，edit 模式下渲染
import { ProcessEditor } from '@/components/process/ProcessEditor';
// M6：新建工艺元信息 Modal
import { CreateProcessMetaModal } from '@/components/process/CreateProcessMetaModal';
// 027：工艺分享到官方仓库
import { ShareToRepoButton, toHomePath } from '@/components/settings/contribute/ShareToRepoButton';
import { buildProcessContributePrompt } from '@/components/settings/contribute/contributePrompts';
// 029：pushUrl 用于"创建工艺"按钮导航到编辑器路由（/#/processes?processMode=new）。
// 109：listView/replaceUrl 用于「我的/模板」形态直达路由（?view=mine|template）。
import { useViewState, pickListView } from '@/hooks/useViewState';

const { Title, Text, Paragraph } = Typography;

/**
 * 039：工艺列表「我的/模板」视图范围。
 * - `mine`：用户自己创建的工艺（is_system=false）
 * - `template`：系统内置及 bundled 同步下载的工艺（is_system=true）
 */
type ProcessScope = 'mine' | 'template';

// localStorage key 带 ntd_ 前缀与项目其他键（如 app_theme）区分业务域，避免冲突。
const SCOPE_STORAGE_KEY = 'ntd_process_list_scope';

/**
 * 读取上次的视图选择；非法值或存储不可用（隐私模式）时回退 'mine'——
 * 用户日常高频操作是管理自己创建的工艺，默认落在「我的」认知成本最低。
 */
function readInitialScope(): ProcessScope {
  try {
    return localStorage.getItem(SCOPE_STORAGE_KEY) === 'template' ? 'template' : 'mine';
  } catch {
    return 'mine';
  }
}

interface ProcessPageProps {
  workspaceId: number | null;
  /** 安装成功后跳转新环路详情（「工艺 → 环路」实例化关系显性化的关键一跳）。 */
  onOpenLoop?: (loopId: number) => void;
  /**
   * URL 携带的工艺模板 guid（`/#/processes?guid=xxx`）。
   * 用于「环路详情 → 来源工艺」回跳后自动打开该工艺详情，形成溯源闭环。
   * 040 起用 guid：name 允许重复（模板与用户副本同名），只有 guid 能精确定位。
   */
  processGuid?: string | null;
  /**
   * 029：工艺编辑器模式。
   * - `'list'`（默认）：渲染工艺列表页
   * - `'new'`：渲染编辑器空白态，先弹元信息 Modal
   * - `'edit'`：渲染编辑器，加载 `processGuid` 对应 YAML
   */
  processMode?: 'list' | 'new' | 'edit';
}

export function ProcessPage({ workspaceId, onOpenLoop, processGuid, processMode = 'list' }: ProcessPageProps) {
  // 029：顶层分流，避免在 if return 后写 useState 造成条件 hooks。
  // 列表态走 ProcessListView（内部自取 useViewState），编辑器态走 ProcessEditor/占位。
  // M3：edit 模式接真实 ProcessEditor，new 模式留占位给 M6
  if (processMode === 'edit' && processGuid) {
    return <ProcessEditor processGuid={processGuid} />;
  }
  if (processMode === 'new') {
    return <ProcessEditorPlaceholder mode="new" name={null} />;
  }
  // 109：ProcessListView 内部自行取 useViewState（pushUrl/listView/replaceUrl），不再由父级注入。
  return <ProcessListView workspaceId={workspaceId} onOpenLoop={onOpenLoop} processGuid={processGuid} />;
}

/**
 * 029：工艺编辑器占位骨架。
 *
 * M3-M6 阶段会被真正的 ProcessEditor 替换：
 * - M3：Monaco YAML 编辑器
 * - M4：React Flow 泳道可视化编辑器
 * - M5：双向联动 + 保存
 * - M6：新建工艺元信息 Modal + 空工艺渲染
 *
 * 当前仅渲染占位 UI，让路由入口可验证。
 */
function ProcessEditorPlaceholder({ mode, name }: { mode: 'new' | 'edit'; name: string | null }) {
  return (
    <div style={{ padding: 60, textAlign: 'center', color: 'var(--color-text-tertiary)' }}>
      <Title level={4}>
        {mode === 'new' ? '创建新工艺（编辑器开发中）' : `编辑工艺：${name ?? '未知'}（编辑器开发中）`}
      </Title>
      <Text type="secondary">
        029 工艺编辑器正在开发中（M3-M6 阶段填充 Monaco YAML 编辑器 + React Flow 泳道可视化）。
      </Text>
    </div>
  );
}

/**
 * 工艺列表视图（原 ProcessPage 的列表逻辑，029 抽为子组件以配合 mode 分流）。
 *
 * 保留原有的工艺模板库列表、查看详情、安装为 Loop 等功能。
 * 029 新增：接收 pushUrl 用于"创建工艺"按钮导航到编辑器路由。
 * 109：scope（我的/模板）改为 URL ?view= 优先 + localStorage 兜底，支持形态直达。
 */
function ProcessListView({ workspaceId, onOpenLoop, processGuid }: Omit<ProcessPageProps, 'processMode'>) {
  const { pushUrl, listView, replaceUrl } = useViewState();
  const [processes, setProcesses] = useState<ProcessTemplate[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  // 040：正在复制的工艺 guid（控制卡片「复制」按钮 loading 态）
  const [copying, setCopying] = useState<string | null>(null);
  const [searchText, setSearchText] = useState('');
  const [recommended, setRecommended] = useState<string[]>([]);
  const [installModal, setInstallModal] = useState<{ guid: string; displayName: string } | null>(null);
  const [instanceLoops, setInstanceLoops] = useState<ProcessLoopItem[]>([]);
  const [instanceLoopsLoading, setInstanceLoopsLoading] = useState(false);
  // 正在升级的环路 id（控制按钮 loading 态）
  const [upgradingLoopId, setUpgradingLoopId] = useState<number | null>(null);
  // M6：新建工艺元信息 Modal 开关
  const [createModalOpen, setCreateModalOpen] = useState(false);
  // 039：「我的/模板」视图范围：URL ?view= 优先（直达指定范围），无参数/非法值
  // 回退 localStorage 记忆（持久化用户上次选择）。storedScope 只在挂载时读一次。
  const [storedScope] = useState<ProcessScope>(readInitialScope);
  // 泛型版 pickListView 由 allowed/fallback 推导 ProcessScope，无需 as 断言
  const scope = pickListView(listView, ['mine', 'template'], storedScope);

  const load = async () => {
    setLoading(true);
    try {
      // 服务端按视图过滤（039 需求明确服务端过滤，不做全量拉取+客户端 filter）：
      // 模板视图只取系统工艺，我的视图只取用户工艺。
      const list = await bundledApi.getProcesses(scope === 'template');
      setProcesses(list);
    } catch (e) {
      message.error('加载工艺模板失败');
    } finally {
      setLoading(false);
    }
  };

  // 依赖 scope：切换视图即重新拉取对应子集，保证列表与 Segmented 选中态一致。
  useEffect(() => {
    load();
  }, [scope]);

  // 切换视图：写 localStorage 兜底 + replaceUrl 同步 URL（?view=），使范围可直达/分享。
  const handleScopeChange = (value: string | number) => {
    const next = value === 'template' ? 'template' : 'mine';
    try {
      localStorage.setItem(SCOPE_STORAGE_KEY, next);
    } catch {
      // 忽略持久化失败
    }
    replaceUrl('processes', { view: next });
  };

  const handleSearch = async (value: string) => {
    setSearchText(value);
    if (!value.trim()) { setRecommended([]); return; }
    try {
      const resp = await bundledApi.recommendProcesses(value);
      // 040：推荐高亮按 guid 匹配——name 允许重复后，按 name 会把同名模板一起点亮。
      setRecommended(resp.recommendations.map((r) => r.template_guid));
    } catch {
      // 推荐失败不影响正常使用。
    }
  };

  const handleShowDetail = async (guid: string) => {
    try {
      const data = await bundledApi.getProcess(guid);
      setDetail(data);
    } catch (e) {
      message.error('加载工艺模板详情失败');
    }
  };

  const fetchInstanceLoops = async (templateGuid: string) => {
    setInstanceLoopsLoading(true);
    try {
      const list = await bundledApi.listProcessLoops(templateGuid);
      setInstanceLoops(list);
    } catch {
      // 实例环路列表加载失败（如该工艺尚未安装）不弹框，静默空列表。
      setInstanceLoops([]);
    } finally {
      setInstanceLoopsLoading(false);
    }
  };

  // URL 携带 guid 参数时自动打开详情：环路详情「来源工艺」回跳的目标落地。
  // cancelled 防御快速切换路由造成的竞态：晚返回的请求发现已卸载/切换就丢弃。
  useEffect(() => {
    if (!processGuid) return;
    let cancelled = false;
    bundledApi.getProcess(processGuid)
      .then((data) => { if (!cancelled) setDetail(data); })
      .catch(() => { if (!cancelled) message.error('加载工艺模板详情失败'); });
    return () => { cancelled = true; };
  }, [processGuid]);

  // 升级实例环路到最新版本：调 API 后刷新实例列表
  const handleUpgradeLoop = async (templateGuid: string, loopId: number) => {
    setUpgradingLoopId(loopId);
    try {
      const result = await bundledApi.upgradeProcessLoop(templateGuid, loopId);
      message.success(`已升级到 ${result.loop_name}（${result.phase_count} 阶段 / ${result.step_count} 步骤）`);
      // 刷新实例环路列表
      await fetchInstanceLoops(templateGuid);
    } catch {
      message.error('升级失败');
    } finally {
      setUpgradingLoopId(null);
    }
  };

  const handleInstall = (guid: string, displayName: string) => {
    setInstallModal({ guid, displayName });
  };

  // 040：把模板复制为我的工艺——后端纯文件复制 + 副本换新 guid，原模板保留。
  // 成功后切到「我的」视图，用户立即看到新卡片并可点编辑，形成「模板 → 可编辑副本」闭环。
  const handleCopyToUser = async (guid: string) => {
    setCopying(guid);
    try {
      const result = await bundledApi.copyProcessToUser(guid);
      message.success(`已复制为我的工艺：${result.name}`);
      handleScopeChange('mine');
    } catch {
      message.error('复制失败');
    } finally {
      setCopying(null);
    }
  };

  const doInstall = async () => {
    if (!installModal || !workspaceId) { message.warning('请先在左上角选择工作空间'); return; }
    setInstalling(installModal.guid);
    try {
      const result = await bundledApi.installProcess(installModal.guid, workspaceId);
      message.success(`已安装「${result.loop_name}」，正在打开环路详情…`);
      setInstallModal(null);
      // 安装即实例化：直接跳到新环路详情，让用户立即看到工艺的运行时形态；
      // 未注入导航回调（如旧调用方）时保持原有停留行为，不破坏兼容性。
      if (onOpenLoop) {
        setDetail(null);
        onOpenLoop(result.loop_id);
      }
    } catch { message.error('安装失败'); }
    finally { setInstalling(null); }
  };

  const complexityColor = (complexity: string) => {
    switch (complexity) {
      case 'light':
        return 'green';
      case 'standard':
        return 'blue';
      case 'complex':
        return 'purple';
      default:
        return 'default';
    }
  };

  const complexityLabel = (complexity: string) => {
    switch (complexity) {
      case 'light':
        return '轻量';
      case 'standard':
        return '标准';
      case 'complex':
        return '复杂';
      default:
        return complexity;
    }
  };

  // 单张工艺卡片渲染（039 从网格 JSX 中抽出，控制列表渲染块的嵌套层级与函数长度）。
  // 040：寻址/高亮统一按 guid（name 可重复）；系统工艺展示「复制」按钮把模板转成可编辑的我的工艺。
  // 工艺卡片点击体验：整卡可点——点击卡片主体等同点「详情」按钮弹出详情 Modal。
  // 底部 actions 区（详情/安装/复制/编辑/分享）的按钮点击会冒泡到卡片 onClick，
  // 必须用 .ant-card-actions 守卫放行，否则点「安装」会同时弹详情 Modal 互相遮挡。
  const renderProcessCard = (p: ProcessTemplate) => (
    <Card
      hoverable
      style={{ flex: 1 }}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('.ant-card-actions')) return;
        handleShowDetail(p.guid);
      }}
      title={p.display_name || p.name}
      extra={
        <Space>
          {recommended.includes(p.guid) && <Tag color="gold">推荐</Tag>}
          <Tag color={complexityColor(p.complexity)}>{complexityLabel(p.complexity)}</Tag>
        </Space>
      }
      actions={[
        <Button key="view" type="text" icon={<EyeOutlined />} onClick={() => handleShowDetail(p.guid)}>
          详情
        </Button>,
        <Button
          key="install"
          type="text"
          icon={<DownloadOutlined />}
          loading={installing === p.guid}
          onClick={() => handleInstall(p.guid, p.display_name || p.name)}
        >
          安装
        </Button>,
        /* 040：系统工艺给「复制为我的工艺」入口（040 的初心交互）；
           用户工艺（我的视图）给「编辑」入口直接进编辑器。两者互斥，按 is_system 二选一。 */
        p.is_system ? (
          <Button
            key="copy"
            type="text"
            icon={<CopyOutlined />}
            loading={copying === p.guid}
            onClick={() => handleCopyToUser(p.guid)}
          >
            复制
          </Button>
        ) : (
          <>
            <Button
              key="edit"
              type="text"
              icon={<EditOutlined />}
              onClick={() => pushUrl('processes', { processMode: 'edit', guid: p.guid })}
            >
              编辑
            </Button>
            {/* 分享仅对用户工艺开放（is_system=false）；source_path 为空（异常/旧数据）时不渲染，
                避免空路径给 AI 执行器。与模板管理-工艺 Tab 的分享参数一致。 */}
            {p.source_path && (
              <span key="share">
                <ShareToRepoButton
                  actionType="process_contribute"
                  actionKey={p.guid}
                  params={{
                    resource_name: p.name,
                    version: p.version,
                    resource_dir: toHomePath(p.source_path),
                    remote_path: `processes/${p.category ? p.category + '/' : ''}${p.name}.yaml`,
                  }}
                  buildPrompt={buildProcessContributePrompt}
                  panelTitle={`分享工艺 ${p.display_name || p.name}`}
                  panelDescription="AI 将读取本机 PAT，把该工艺 YAML 提交为 PR 到官方仓库（可编辑下方 Prompt）"
                  size="small"
                  iconOnly
                />
              </span>
            )}
          </>
        ),
      ]}
    >
      <Paragraph ellipsis={{ rows: 2 }} type="secondary">
        {p.description || '无描述'}
      </Paragraph>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <Text type="secondary" style={{ fontSize: 12 }}>
          版本 {p.version}
        </Text>
        <Tag>{p.category || '未分类'}</Tag>
      </div>
    </Card>
  );

  return (
    <PageCard
      icon={<BuildOutlined />}
      title="工艺"
      extra={
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={() => setCreateModalOpen(true)}>
            创建工艺
          </Button>
        </div>
      }
      style={{ flex: 1, height: '100%' }}
    >

      {/* 039：「我的/模板」视图切换，独占一行置于搜索栏上方；
          用 Segmented 而非 Tabs 是因项目内同列表视图切换均用 Segmented（SkillsPanel 等），视觉更轻量。
          109：形态受 URL ?view= 驱动（mine|template），testid 供 Playwright 验证直达形态。 */}
      <div style={{ marginBottom: 12 }}>
        <Segmented<ProcessScope>
          value={scope}
          onChange={handleScopeChange}
          data-testid="process-scope-toggle"
          options={[
            { label: '我的', value: 'mine' },
            { label: '模板', value: 'template' },
          ]}
        />
      </div>

      <div style={{ display: 'flex', gap: 12, marginBottom: 16 }}>
        <Input.Search
          placeholder="搜索或描述任务，系统推荐合适工艺…"
          allowClear
          enterButton={<><SearchOutlined /> 推荐</>}
          size="large"
          onSearch={handleSearch}
          onChange={(e) => { if (!e.target.value) setRecommended([]); }}
          style={{ flex: 1 }}
        />
        <Select
          placeholder="复杂度筛选"
          allowClear
          style={{ width: 140 }}
          onChange={(val) => { setSearchText(val ? '' : searchText); }}
          options={[
            { label: '轻量', value: 'light' },
            { label: '标准', value: 'standard' },
            { label: '复杂', value: 'complex' },
          ]}
        />
      </div>

      {recommended.length > 0 && (
        <Alert
          type="info"
          showIcon
          message="系统推荐"
          description={`推荐使用：${recommended.join('、')}`}
          style={{ marginBottom: 16 }}
          closable
          onClose={() => setRecommended([])}
        />
      )}

      {loading && processes.length === 0 ? (
        <div style={{ textAlign: 'center', padding: 64 }}>
          <Spin size="large" />
        </div>
      ) : processes.length === 0 ? (
        // 空态按视图给出不同引导：「我的」空引导创建，「模板」空引导同步——
        // 两者的修复动作不同，统一文案会把用户误导到错误的入口。
        <Empty
          description={
            scope === 'mine'
              ? '暂无自定义工艺，可点击右上角「创建工艺」'
              : '暂无系统模板，请先在「更多设置 → 模板管理」执行同步'
          }
        />
      ) : (
        <Row gutter={[16, 16]}>
          {/* 搜索过滤保持客户端：列表已是当前视图的服务端过滤子集，
              搜索词在此子集上再过滤即可；切换视图不清空搜索词，避免用户输入丢失。 */}
          {processes
            .filter((p) => !searchText.trim() || p.name.includes(searchText) || p.display_name.includes(searchText) || p.description.includes(searchText))
            .map((p) => (
            <Col xs={24} sm={12} lg={8} key={p.id} style={{ display: 'flex' }}>
              {renderProcessCard(p)}
            </Col>
          ))}
        </Row>
      )}

      <Modal
        open={detail != null}
        title={detail?.display_name || detail?.name}
        width={960}
        centered
        footer={[
          <Button key="close" onClick={() => setDetail(null)}>
            关闭
          </Button>,
          <Button
            key="install"
            type="primary"
            icon={<DownloadOutlined />}
            loading={installing === detail?.guid}
            disabled={!workspaceId}
            onClick={() => detail && handleInstall(detail.guid, detail.display_name || detail.name)}
          >
            安装到当前工作空间
          </Button>,
        ]}
        onCancel={() => setDetail(null)}
      >
        {detail && (
          <Tabs
            defaultActiveKey="flow"
            onChange={(key) => {
              if (key === 'instances') {
                fetchInstanceLoops(detail.guid);
              }
            }}
            items={[
              {
                key: 'flow',
                label: <span><ApartmentOutlined /> 流程图</span>,
                children: (() => {
                  const adapted = adaptProcessDefinition(detail.definition);
                  if (!adapted) {
                    return <div style={{ color: 'var(--color-text-tertiary)', textAlign: 'center', padding: 60 }}>该工艺定义无法解析，请查看 YAML 源排查语法</div>;
                  }
                  return (
                    <div>
                      <Descriptions size="small" column={2} style={{ marginBottom: 12 }}>
                        <Descriptions.Item label="名称">{detail.name}</Descriptions.Item>
                        <Descriptions.Item label="版本">{detail.version}</Descriptions.Item>
                        <Descriptions.Item label="分类">{detail.category}</Descriptions.Item>
                        <Descriptions.Item label="复杂度">
                          <Tag color={complexityColor(detail.complexity)}>{complexityLabel(detail.complexity)}</Tag>
                        </Descriptions.Item>
                      </Descriptions>
                      <Paragraph type="secondary">{detail.description}</Paragraph>
                      <ProcessFlowGraph
                        links={adapted.links}
                        nodeInputs={adapted.nodeInputs}
                        edgeInputs={adapted.edgeInputs}
                        templateEdges={adapted.templateEdges}
                        phaseGroups={adapted.phaseGroups}
                      />
                    </div>
                  );
                })(),
              },
              {
                key: 'instances',
                label: '实例环路',
                children: instanceLoopsLoading ? (
                  <div style={{ textAlign: 'center', padding: 40 }}><Spin size="small" /></div>
                ) : instanceLoops.length === 0 ? (
                  <Empty description="该工艺尚未安装到任何工作空间" />
                ) : (
                  <Table<ProcessLoopItem>
                    dataSource={instanceLoops}
                    rowKey="id"
                    size="small"
                    pagination={false}
                    columns={[
                      { title: '名称', dataIndex: 'name', key: 'name' },
                      { title: '状态', dataIndex: 'status', key: 'status', render: (s: string) => <Tag color={s === 'active' ? 'green' : 'default'}>{s}</Tag> },
                      { title: '版本', dataIndex: 'process_template_version', key: 'version', render: (v: string | null) => v || '-' },
                      { title: '执行次数', dataIndex: 'execution_count', key: 'count' },
                      {
                        title: '', key: 'action', render: (_: unknown, rec: ProcessLoopItem) => (
                          <Space size={4}>
                            <Button type="link" size="small" icon={<EyeOutlined />}
                              onClick={() => {
                                setDetail(null);
                                onOpenLoop?.(rec.id);
                              }}
                            >打开</Button>
                            <Button
                              type="link" size="small"
                              loading={upgradingLoopId === rec.id}
                              onClick={() => handleUpgradeLoop(detail!.guid, rec.id)}
                            >更新</Button>
                          </Space>
                        ),
                      },
                    ]}
                  />
                ),
              },
              {
                key: 'yaml',
                label: <span><CodeOutlined /> YAML 源</span>,
                children: (
                  <div>
                    {/* 029：用户工艺可编辑，跳编辑器路由；系统工艺只读（编辑了会被同步覆盖）。 */}
                    {detail && !detail.is_system && (
                      <div style={{ marginBottom: 12 }}>
                        <Button
                          type="primary"
                          icon={<EditOutlined />}
                          onClick={() => {
                            setDetail(null);
                            pushUrl('processes', { processMode: 'edit', guid: detail.guid });
                          }}
                        >
                          编辑工艺
                        </Button>
                      </div>
                    )}
                    {detail && detail.is_system && (
                      // 040：系统工艺警示条旁直接给「复制为我的工艺」按钮——
                      // 旧版文案写着"请先复制到用户层"却没按钮，交互是断的，这里补上。
                      <Alert
                        type="warning"
                        showIcon
                        message="系统工艺编辑后会被同步覆盖"
                        description={
                          <Button
                            type="link"
                            icon={<CopyOutlined />}
                            style={{ padding: 0 }}
                            loading={copying === detail.guid}
                            onClick={() => detail && handleCopyToUser(detail.guid)}
                          >
                            复制为我的工艺后编辑
                          </Button>
                        }
                        style={{ marginBottom: 12 }}
                      />
                    )}
                    <pre style={{
                      // YAML 源码块底色用主题填充色：亮色 4% 黑，暗色切到 surface 灰
                      background: 'var(--color-fill-tertiary)',
                      padding: 12,
                      borderRadius: 8,
                      maxHeight: 400,
                      overflow: 'auto',
                      fontSize: 12,
                      margin: 0,
                    }}>
                      <code>{detail.definition}</code>
                    </pre>
                  </div>
                ),
              },
            ]}
          />
        )}
      </Modal>

      <Modal
        title={`安装工艺模板`}
        open={!!installModal}
        onCancel={() => setInstallModal(null)}
        onOk={doInstall}
        confirmLoading={!!installing}
        okText="安装"
      >
        将「{installModal?.displayName || ''}」安装到当前工作空间？
        {!workspaceId && <div style={{ color: 'red', marginTop: 8 }}>请先在页面左上角选择目标工作空间</div>}
      </Modal>

      {/* M6：新建工艺元信息 Modal，确认后跳编辑器 */}
      <CreateProcessMetaModal
        open={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        onCreated={(guid) => {
          setCreateModalOpen(false);
          // 040：跳路由进编辑器，按 guid 定位新工艺（name 不再唯一）
          pushUrl('processes', { processMode: 'edit', guid });
        }}
      />
    </PageCard>
  );
}
