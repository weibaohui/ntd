// 新建任务 Modal（需求 092 起支持两种执行方式）。
// - 工艺环路（默认）：填需求 + 选环路，后端按预设流程多环节执行（与历史行为一致）。
// - 委派：填需求 + 选处理人（专家/执行器），可选开启自动接力（仅专家）；
//   后端建无环路 task 并在讨论区落 @处理人 首帖触发首次执行（复用 060 @ 机制）。
// API：bundledApi.createTask(params, wsId)，按 executionMode 组装不同参数。
// 提交成功后调 onCreated，宿主关闭 Modal 并刷新列表。

import { useEffect, useState } from 'react';
import { Modal, Input, InputNumber, Select, Form, Radio, Segmented, Switch, Typography, message } from 'antd';
import { ThunderboltOutlined } from '@ant-design/icons';
import bundledApi from '@/api/bundled';
import type { LoopLite } from '@/components/tasks/constants';
import { loopOptionLabel } from '@/components/tasks/constants';
import { getAllExperts } from '@/utils/database/experts';
import { EXECUTORS_FOR_PICKER } from '@/utils/executors';
import { getExpertDisplayName } from '@/types/expert';
import type { ExpertMetadata } from '@/types/expert';
import { getWorkspaceSettings } from '@/utils/database/bots';

const { Text } = Typography;

/** 表单字段类型。executionMode 决定渲染哪一组字段与必填校验。 */
interface CreateTaskFormValues {
  executionMode: 'loop' | 'delegate';
  requirement: string;
  loopId?: number;
  assigneeKind?: 'executor' | 'expert';
  assigneeName?: string;
  autoContinue?: boolean;
  // 接力轮数上限覆盖：null=沿用工作空间默认（提交 omit）；N=任务级覆盖（1..=50）。
  delegateMaxRounds?: number | null;
}

interface CreateTaskModalProps {
  open: boolean;
  workspaceId: number;
  loops: LoopLite[];
  onCreated: () => void;
  onCancel: () => void;
}

/**
 * 新建任务 Modal。
 *
 * 处理流程：
 *   1. 选执行方式（工艺环路 / 委派），按选择联动显示对应表单。
 *   2. 填需求 +（环路模式）选环路 /（委派模式）选处理人。
 *   3. 点「开始执行」→ 校验 → createTask API。
 *   4. 成功 → message.success + onCreated；失败 → message.error，Modal 不关。
 *
 * Form.useWatch 监听 executionMode/assigneeKind，据此条件渲染字段、联动校验与开关可用性，
 * 避免 antd Form 跨字段静态 rules 的样板。
 */
export function CreateTaskModal({
  open,
  workspaceId,
  loops,
  onCreated,
  onCancel,
}: CreateTaskModalProps) {
  // antd Form 实例：用 Form.useForm 获取，便于关闭时重置。
  const [form] = Form.useForm<CreateTaskFormValues>();
  // 提交 loading 态：防止重复点击。
  const [submitting, setSubmitting] = useState(false);
  // 监听执行方式与处理人类型，驱动条件渲染与联动（未初始化时用兜底值）。
  const executionMode = Form.useWatch('executionMode', form) ?? 'loop';
  const assigneeKind = Form.useWatch('assigneeKind', form) ?? 'expert';
  // 监听自动接力开关：仅 expert + 开启时才需配置/展示「最大轮数」字段。
  const autoContinue = Form.useWatch('autoContinue', form) ?? false;
  // 专家候选：打开 Modal 时拉一次（执行器候选是静态 EXECUTORS_FOR_PICKER，无需拉取）。
  const [experts, setExperts] = useState<ExpertMetadata[]>([]);
  // 专家下拉加载态：加载中给 Select 转圈，失败时弹 message.error（而非只 console.warn）。
  const [expertsLoading, setExpertsLoading] = useState(false);
  // 工作空间「接力上限默认」有效值：用作「最大轮数」placeholder 与开关 tooltip，前端不硬编码 10。
  const [wsDefaultMaxRounds, setWsDefaultMaxRounds] = useState<number>(10);
  useEffect(() => {
    if (!open) return;
    // 失败时若只 console.warn，用户只看到空下拉「暂无可用专家」，无从判断是真空还是加载失败（CodeRabbit #8）。
    setExpertsLoading(true);
    getAllExperts()
      .then(setExperts)
      .catch((e) => {
        console.warn('专家列表加载失败', e);
        message.error('专家列表加载失败，请稍后重试');
        setExperts([]);
      })
      .finally(() => setExpertsLoading(false));
  }, [open]);

  // 取工作空间「接力上限默认」有效值（raw null → 后端已回退兜底 10），仅用于 placeholder/tooltip；
  // 失败回退本地兜底 10，不阻断创建（创建本身不依赖此值，留空即用默认）。
  useEffect(() => {
    if (!open) return;
    getWorkspaceSettings(workspaceId)
      .then((s) => setWsDefaultMaxRounds(s.delegate_max_rounds_effective ?? 10))
      .catch(() => setWsDefaultMaxRounds(10));
  }, [open, workspaceId]);

  // open 变为 false 时重置表单字段，避免下次打开残留上次输入。
  useEffect(() => {
    if (!open) {
      form.resetFields();
    }
  }, [open, form]);

  // 提交处理：按 executionMode 组装 createTask 参数（环路/委派两套字段互不混传）。
  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      setSubmitting(true);
      // 委派模式不传 loopId；自动接力仅专家允许（执行器在前端已禁用，这里再收口一次防绕过）。
      const result =
        values.executionMode === 'delegate'
          ? await bundledApi.createTask(
              {
                requirement: values.requirement,
                executionMode: 'delegate',
                assigneeKind: values.assigneeKind,
                assigneeName: values.assigneeName,
                autoContinue: values.assigneeKind === 'expert' ? values.autoContinue : false,
                // 仅在用户填了上限时下发覆盖；null/undefined → createTask omit → 后端用工作空间默认。
                delegateMaxRounds: values.delegateMaxRounds ?? undefined,
              },
              workspaceId,
            )
          : await bundledApi.createTask(
              { requirement: values.requirement, loopId: values.loopId },
              workspaceId,
            );
      // 委派首帖触发失败时后端仍建好任务（execution_id=null），但不能用 success 误报「已开跑」：
      // 改用 warning 明确告知「任务建好但执行没起来，需手动 @」，避免用户空等（CodeRabbit #9）。
      if (values.executionMode === 'delegate' && !result.execution_id) {
        message.warning('任务已创建，但首次执行未能触发，请到讨论区手动 @ 处理人');
      } else {
        message.success(result.execution_id ? `任务已创建，执行 #${result.execution_id}` : '任务已创建');
      }
      onCreated();
    } catch (err) {
      // validateFields 失败时不报错；只有 API 失败才报错。
      if (err instanceof Error) {
        message.error(`创建任务失败：${err.message}`);
      }
    } finally {
      setSubmitting(false);
    }
  };

  // 委派处理人下拉候选：按处理人类型切换专家/执行器两套数据源。
  const assigneeOptions =
    assigneeKind === 'expert'
      ? // 专家：value 用规范名 name（后端按 expert.name 匹配），label 用展示名。
        experts.map((e) => ({ label: getExpertDisplayName(e), value: e.name }))
      : // 执行器：value 用规范名（如 codex），后端 find_executor 据此匹配。
        EXECUTORS_FOR_PICKER.map((e) => ({ label: e.label, value: e.value }));

  return (
    <Modal
      title={
        <span>
          <ThunderboltOutlined style={{ marginRight: 8 }} />
          新建任务
        </span>
      }
      open={open}
      onCancel={onCancel}
      onOk={handleSubmit}
      confirmLoading={submitting}
      okText="开始执行"
      cancelText="取消"
      width={560}
      destroyOnClose
      data-testid="create-task-modal"
    >
      <Form
        form={form}
        layout="vertical"
        requiredMark
        initialValues={{ executionMode: 'loop', assigneeKind: 'expert', autoContinue: false }}
      >
        {/* 执行方式：默认工艺环路（与历史一致），切换后联动下方字段。 */}
        <Form.Item name="executionMode" label="执行方式">
          <Radio.Group optionType="button" buttonStyle="solid" data-testid="create-task-mode">
            <Radio.Button value="loop">工艺环路</Radio.Button>
            <Radio.Button value="delegate">委派</Radio.Button>
          </Radio.Group>
        </Form.Item>

        <Form.Item
          name="requirement"
          label="需求描述"
          rules={[
            { required: true, message: '请输入需求描述' },
            { min: 4, message: '需求描述至少 4 个字符' },
          ]}
        >
          <Input.TextArea
            placeholder="我想做什么？例如：把这段 Rust 代码改成异步实现"
            rows={4}
            maxLength={2000}
            showCount
            data-testid="create-task-requirement"
          />
        </Form.Item>

        {/* 工艺环路模式：选环路（必填）。条件渲染使该字段在委派模式下不参与校验。 */}
        {executionMode === 'loop' ? (
          <Form.Item
            name="loopId"
            label="工艺环路"
            rules={[{ required: true, message: '请选择工艺环路' }]}
            extra={
              loops.length === 0
                ? '当前工作空间暂无工艺环路，请先在「环路」页创建'
                : undefined
            }
          >
            <Select
              placeholder="选择一个工艺环路"
              options={loops.map((l) => ({
                // label 由 loopOptionLabel 拼装：#环路ID 名称（#工艺ID 工艺名 版本）。
                label: loopOptionLabel(l),
                value: l.id,
              }))}
              notFoundContent="暂无可用工艺环路"
              data-testid="create-task-loop-select"
            />
          </Form.Item>
        ) : (
          <>
            {/* 委派模式：处理人类型 + 处理人选择 + 自动接力开关。 */}
            <Form.Item name="assigneeKind" label="处理人类型">
              <Segmented
                options={[
                  { label: '专家', value: 'expert' },
                  { label: '执行器', value: 'executor' },
                ]}
                // 切换处理人类型时清空已选处理人：专家名与执行器名是不同命名空间，
                // 保留旧值会把专家名当执行器名（或反之）提交，后端校验必失败（CodeRabbit #10）。
                onChange={() => form.setFieldValue('assigneeName', undefined)}
              />
            </Form.Item>
            <Form.Item
              name="assigneeName"
              label="处理人"
              rules={[{ required: true, message: '请选择处理人' }]}
            >
              <Select
                placeholder={assigneeKind === 'expert' ? '选择一个专家' : '选择一个执行器'}
                options={assigneeOptions}
                showSearch
                optionFilterProp="label"
                // 仅专家候选需异步加载，执行器是静态列表；加载中转圈提示用户等待。
                loading={assigneeKind === 'expert' && expertsLoading}
                notFoundContent={assigneeKind === 'expert' ? '暂无可用专家' : '暂无可用执行器'}
                data-testid="create-task-assignee"
              />
            </Form.Item>
            {/* 自动接力仅专家可用：执行器是 CLI 无调度能力，禁用开关并说明原因（后端亦 400 双重校验）。 */}
            <Form.Item
              name="autoContinue"
              label="自动接力"
              tooltip={`开启后，每次执行完成由该专家自主决定下一步（管家模式），直到完成或达 ${wsDefaultMaxRounds} 轮上限。仅专家支持。`}
              valuePropName="checked"
            >
              <Switch disabled={assigneeKind === 'executor'} data-testid="create-task-auto-continue" />
            </Form.Item>
            {assigneeKind === 'executor' ? (
              <Text type="secondary" style={{ fontSize: 12 }}>
                执行器不支持自动接力（如需托管调度，请改选专家）
              </Text>
            ) : null}
            {/* 接力轮数上限覆盖：仅专家 + 自动接力开启时才需配（否则无接力可言）。
                preserve=false 保证关闭开关/切执行器时字段卸载即清值，避免残留上限被误提交。 */}
            {assigneeKind === 'expert' && autoContinue ? (
              <Form.Item
                name="delegateMaxRounds"
                label="最大轮数"
                tooltip="达此轮数上限后停止自动接力；留空则沿用工作空间默认。"
                preserve={false}
              >
                <InputNumber
                  min={1}
                  max={50}
                  placeholder={`默认 ${wsDefaultMaxRounds} 轮（工作空间配置）`}
                  style={{ width: '100%' }}
                  data-testid="create-task-max-rounds"
                />
              </Form.Item>
            ) : null}
          </>
        )}
      </Form>
    </Modal>
  );
}
