import { useState, useEffect } from 'react';
import { Modal, Form, Input, Button, List, Popconfirm, Space, message } from 'antd';
import { DeleteOutlined, EditOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
import * as db from '@/utils/database';
import type { QuickButton } from '@/utils/database';

/** 示例话术占位符：引导用户加「提取 skill」这类按钮（只是提示，不内置数据） */
const PROMPT_PLACEHOLDER =
  '如：请把刚才这次执行的过程和结论提炼成一个可复用的 skill，写入对应 SKILL.md';

type FormValues = { button_name: string; prompt_text: string };

/** 提交前规整表单值：trim 防止全空格混过前端 required 校验 */
function normalizeFormValues(values: FormValues): FormValues {
  return { button_name: values.button_name.trim(), prompt_text: values.prompt_text.trim() };
}

/**
 * 封装快捷按钮的列表加载 + 增删改 + 编辑态。把数据逻辑从渲染层剥离，
 * 主组件只管布局。弹窗每次打开重拉最新列表，CRUD 后本地刷新并通知外层同步按钮条。
 */
function useQuickButtonCrud(open: boolean, workspaceId: number, onChanged: () => void) {
  const [buttons, setButtons] = useState<QuickButton[]>([]);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form] = Form.useForm<FormValues>();

  const refresh = () =>
    db
      .getQuickButtons(workspaceId)
      .then(setButtons)
      .catch((e: unknown) => message.error('加载快捷按钮失败: ' + String(e)));

  // 弹窗打开时重拉列表 + 回到「新建」态，避免上次编辑残留串到下次
  useEffect(() => {
    if (!open) return;
    refresh();
    setEditingId(null);
    form.resetFields();
    // open 是唯一触发源；form 引用稳定故不列入依赖
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // 进入编辑：回填表单，主组件据此切换按钮文案与高亮
  const beginEdit = (b: QuickButton) => {
    setEditingId(b.id);
    form.setFieldsValue({ button_name: b.button_name, prompt_text: b.prompt_text });
  };

  const cancelEdit = () => {
    setEditingId(null);
    form.resetFields();
  };

  // 新建或更新二合一：editingId 决定走哪条分支
  const submit = async () => {
    try {
      const values = normalizeFormValues(await form.validateFields());
      if (editingId) {
        await db.updateQuickButton(workspaceId, editingId, values);
        message.success('更新成功');
      } else {
        await db.createQuickButton(workspaceId, values);
        message.success('创建成功');
      }
      cancelEdit();
      refresh();
      onChanged();
    } catch (err: unknown) {
      // errorFields 来自表单校验失败（antd 已在字段下提示），这里不重复弹错
      if (!(err as { errorFields?: unknown })?.errorFields) {
        message.error('操作失败: ' + String(err));
      }
    }
  };

  const remove = async (id: number) => {
    try {
      await db.deleteQuickButton(workspaceId, id);
      message.success('删除成功');
      refresh();
      onChanged();
    } catch (err: unknown) {
      message.error('删除失败: ' + String(err));
    }
  };

  return { buttons, editingId, form, submit, remove, beginEdit, cancelEdit };
}

/** 已有按钮列表：每条展示名称 + 话术，提供编辑（回填表单）与删除 */
function ExistingButtonList({
  buttons,
  editingId,
  onEdit,
  onDelete,
}: {
  buttons: QuickButton[];
  editingId: number | null;
  onEdit: (b: QuickButton) => void;
  onDelete: (id: number) => void;
}) {
  return (
    <List
      size="small"
      dataSource={buttons}
      locale={{ emptyText: '暂无快捷按钮，在上方添加第一个' }}
      renderItem={(b) => (
        <List.Item
          // 编辑中的行高亮，提示用户当前正在改哪条
          style={b.id === editingId ? { background: '#e6f4ff' } : undefined}
          actions={[
            <Button key="edit" type="text" size="small" icon={<EditOutlined />} onClick={() => onEdit(b)} />,
            <Popconfirm
              key="del"
              title="确定删除此按钮？"
              onConfirm={() => onDelete(b.id)}
              okText="删除"
              cancelText="取消"
            >
              <Button type="text" size="small" icon={<DeleteOutlined />} />
            </Popconfirm>,
          ]}
        >
          <List.Item.Meta title={b.button_name} description={b.prompt_text} />
        </List.Item>
      )}
    />
  );
}

/**
 * 快捷按钮管理弹窗：上半新建/编辑表单，下半已有按钮列表。
 * 编辑态时表单回填、高亮当前行；保存或取消后回到新建态。
 */
export function QuickButtonManageModal({
  open,
  workspaceId,
  onClose,
  onChanged,
}: {
  open: boolean;
  workspaceId: number;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { buttons, editingId, form, submit, remove, beginEdit, cancelEdit } = useQuickButtonCrud(
    open,
    workspaceId,
    onChanged,
  );
  // 移动端窄屏：Modal 走接近全宽，避免固定 520 溢出视口
  const isMobile = useIsMobile();

  return (
    <Modal
      title={editingId ? '编辑快捷按钮' : '管理快捷按钮'}
      open={open}
      onCancel={onClose}
      footer={null}
      width={isMobile ? '92%' : 520}
      destroyOnClose
    >
      <Form form={form} layout="vertical" style={{ marginTop: 12 }}>
        <Form.Item name="button_name" label="按钮名称" rules={[{ required: true, whitespace: true, message: '请输入按钮名称' }]}>
          <Input placeholder="如：提取skill" maxLength={30} />
        </Form.Item>
        <Form.Item name="prompt_text" label="话术" rules={[{ required: true, whitespace: true, message: '请输入话术' }]}>
          <Input.TextArea rows={3} placeholder={PROMPT_PLACEHOLDER} />
        </Form.Item>
        <Space style={{ marginBottom: 16 }}>
          <Button type="primary" onClick={submit}>
            {editingId ? '保存' : '添加'}
          </Button>
          {editingId && <Button onClick={cancelEdit}>取消编辑</Button>}
        </Space>
      </Form>
      <ExistingButtonList buttons={buttons} editingId={editingId} onEdit={beginEdit} onDelete={remove} />
    </Modal>
  );
}
