// CreateProcessMetaModal.tsx
// ---------------------------------------------------------------------------
// M6 里程碑：新建工艺时的元信息收集 Modal。
//
// 设计意图（对应 docs/design/029-M6-新建工艺流程-方案.md §3.1.1 + 设计 §4.1）：
// - 收集 6 字段元信息（name / display_name / description / category / complexity / version）
// - name 实时校验：非空 + 合法字符（[a-z0-9-]+）+ 唯一性（debounced getProcesses 比对）
// - 确认后调 buildEmptyProcessYaml 构造空工艺 YAML + bundledApi.postProcess 创建
// - 成功后 onCreated(name)，失败保持 Modal 打开让用户改名重试
//
// 数据流：纯展示组件，open/onClose/onCreated 由父 ProcessPage 注入。
// ---------------------------------------------------------------------------

import {
  type JSX,
  useState,
  useEffect,
  useCallback,
} from 'react';
import { Modal, Form, Input, Select, message } from 'antd';
import { bundledApi } from '@/api/bundled';
import { buildEmptyProcessYaml, type ProcessMetaInput } from './buildEmptyProcessYaml';

// 类别选项（与后端 ProcessTemplate.category 对齐，YAGNI 取常见值）
const CATEGORY_OPTIONS = [
  { label: '软件', value: 'software' },
  { label: '研究', value: 'research' },
  { label: '写作', value: 'writing' },
  { label: '其他', value: 'other' },
];

// 复杂度选项（与后端 ProcessTemplate.complexity 对齐）
const COMPLEXITY_OPTIONS = [
  { label: '轻量', value: 'lightweight' },
  { label: '标准', value: 'standard' },
  { label: '复杂', value: 'complex' },
];

export interface CreateProcessMetaModalProps {
  // Modal 是否打开
  open: boolean;
  // 关闭回调（遮罩/取消按钮触发）
  onClose: () => void;
  // 创建成功回调（传入新工艺的 name，供父组件跳路由）
  onCreated: (name: string) => void;
}

// Modal 组件实现。
//
// 内部状态：isSubmitting（禁用按钮 + loading）、existingNames（唯一性校验用）。
// name 唯一性校验：Modal 打开时拉一次列表缓存，避免每次输入都打 API。
export function CreateProcessMetaModal({
  open,
  onClose,
  onCreated,
}: CreateProcessMetaModalProps): JSX.Element {
  const [form] = Form.useForm();
  const [isSubmitting, setIsSubmitting] = useState(false);
  // 已有工艺名缓存，唯�性校验用（Modal 打开时拉一次）
  const [existingNames, setExistingNames] = useState<string[]>([]);

  // Modal 打开时拉工艺列表缓存名，唯�性校验用
  // 依赖 [open]：每次打开都刷新（避免列表页新增工艺后校验过期）
  useEffect(() => {
    if (!open) return;
    const loadExisting = async () => {
      try {
        const list = await bundledApi.getProcesses();
        setExistingNames(list.map((p) => p.name));
      } catch {
        // 拉列表失败时不阻塞用户，唯�性兜底交给后端 POST 的 409
        setExistingNames([]);
      }
    };
    void loadExisting();
  }, [open]);

  // 提交回调：校验通过 → 构造 YAML → POST → 成功 onCreated
  const handleSubmit = useCallback(async () => {
    try {
      const values = await form.validateFields();
      setIsSubmitting(true);
      // 构造元信息输入（空串可选字段让纯函数自动跳过）
      const meta: ProcessMetaInput = {
        name: values.name,
        display_name: values.display_name,
        description: values.description,
        category: values.category,
        complexity: values.complexity,
        version: values.version,
      };
      const yamlText = buildEmptyProcessYaml(meta);
      await bundledApi.postProcess(yamlText);
      message.success('工艺已创建');
      // 成功后重置表单 + 通知父组件跳路由
      form.resetFields();
      onCreated(meta.name);
    } catch (err) {
      // validateFields 抛出的 ValidationError 形如 { errorFields: [...] }，
      // 用 errorFields 判别校验失败（Form 内部已反馈，这里静默退出）
      if (typeof err === 'object' && err !== null && 'errorFields' in err) return;
      // 后端 409（重名）等：保持 Modal 打开，提示用户改名
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`创建失败：${msg}`);
    } finally {
      setIsSubmitting(false);
    }
  }, [form, onCreated]);

  // name 唯一性异步校验：debounce 300ms 后比对 existingNames
  // 用 useCallback 缓存校验函数，Form 的 validator 只在 name 变化时触发
  const validateNameUnique = useCallback(
    // Form 的 validator 传入 value，返回 Promise<void> 或 Promise<string>（错误信息）
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    (_rule: unknown, value: string) => {
      if (!value) return Promise.resolve();
      // 唯一性比对：大小写敏感（name 是路由参数，约定小写）
      if (existingNames.includes(value)) {
        return Promise.reject('工艺已存在，请改名');
      }
      return Promise.resolve();
    },
    [existingNames],
  );

  return (
    <Modal
      title="创建工艺"
      open={open}
      onOk={handleSubmit}
      onCancel={onClose}
      okText="创建"
      cancelText="取消"
      confirmLoading={isSubmitting}
      // 遮罩不关闭：避免误触丢失输入；只能用取消按钮关闭
      maskClosable={false}
      // 销毁后重置表单，避免下次打开残留上次输入
      destroyOnClose
    >
      <Form
        form={form}
        layout="vertical"
        initialValues={{
          category: 'software',
          complexity: 'lightweight',
          version: '1.0.0',
        }}
      >
        <Form.Item
          name="name"
          label="工艺名"
          rules={[
            { required: true, message: '请输入工艺名' },
            {
              pattern: /^[a-z0-9-]+$/,
              message: '只能用小写字母、数字、连字符',
            },
            { validator: validateNameUnique },
          ]}
          // 唯一性校验提示文案
          extra="工艺唯一标识，用于路由与文件名"
        >
          <Input placeholder="如 my-process" autoComplete="off" />
        </Form.Item>

        <Form.Item
          name="display_name"
          label="显示名"
          rules={[{ required: true, message: '请输入显示名' }]}
        >
          <Input placeholder="如 我的工艺" autoComplete="off" />
        </Form.Item>

        <Form.Item name="description" label="描述">
          <Input.TextArea placeholder="工艺用途说明（可空）" rows={2} />
        </Form.Item>

        <Form.Item name="category" label="类别">
          <Select options={CATEGORY_OPTIONS} />
        </Form.Item>

        <Form.Item name="complexity" label="复杂度">
          <Select options={COMPLEXITY_OPTIONS} />
        </Form.Item>

        <Form.Item
          name="version"
          label="版本"
          rules={[{ pattern: /^\d+\.\d+\.\d+$/, message: '请用语义版本格式，如 1.0.0' }]}
        >
          <Input placeholder="1.0.0" autoComplete="off" />
        </Form.Item>
      </Form>
    </Modal>
  );
}
