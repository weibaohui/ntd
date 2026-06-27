import { useState, useEffect } from 'react';
import { Table, Button, Space, Switch, Popconfirm, message, Modal, Form, Input, Select, Radio } from 'antd';
import { PlusOutlined, DeleteOutlined, EditOutlined } from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import * as db from '@/utils/database';
import type { WorkspaceSlashCommand } from '@/utils/database';
import type { Todo } from '@/types';
import type { LoopListItem } from '@/types/loop';

interface WorkspaceSlashCommandsPanelProps {
  workspaceId: number;
  /** 是否显示操作列（如在 workspace 详情页内嵌则传 false） */
  showActions?: boolean;
  /** 当 slash command 列表变化时的回调 */
  onChanged?: () => void;
}

export function WorkspaceSlashCommandsPanel({
  workspaceId,
  showActions = true,
  onChanged,
}: WorkspaceSlashCommandsPanelProps) {
  const [commands, setCommands] = useState<WorkspaceSlashCommand[]>([]);
  const [loading, setLoading] = useState(false);
  const [todos, setTodos] = useState<Todo[]>([]);
  const [loops, setLoops] = useState<LoopListItem[]>([]);

  // Modal 状态
  const [modalVisible, setModalVisible] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form] = Form.useForm();

  // 当前选中的命令类型
  const commandType = Form.useWatch('command_type', form) as 'todo' | 'loop' | undefined;

  const loadCommands = () => {
    setLoading(true);
    db.getWorkspaceSlashCommands(workspaceId)
      .then(setCommands)
      .catch((err: any) => message.error('加载斜杠命令失败: ' + (err?.message || String(err))))
      .finally(() => setLoading(false));
  };

  const loadTodos = () => {
    db.getAllTodos(workspaceId).then(setTodos).catch(() => {});
  };

  const loadLoops = () => {
    db.listLoops(workspaceId).then(setLoops).catch(() => {});
  };

  useEffect(() => {
    loadCommands();
    loadTodos();
    loadLoops();
  }, [workspaceId]);

  const handleToggle = async (cmd: WorkspaceSlashCommand, enabled: boolean) => {
    try {
      await db.updateWorkspaceSlashCommand(workspaceId, cmd.id, { enabled });
      setCommands(prev => prev.map(c => c.id === cmd.id ? { ...c, enabled } : c));
      message.success(enabled ? '已启用' : '已禁用');
    } catch (err: any) {
      message.error('操作失败: ' + (err?.message || String(err)));
    }
  };

  const handleDelete = async (cmdId: number) => {
    try {
      await db.deleteWorkspaceSlashCommand(workspaceId, cmdId);
      setCommands(prev => prev.filter(c => c.id !== cmdId));
      message.success('删除成功');
      onChanged?.();
    } catch (err: any) {
      message.error('删除失败: ' + (err?.message || String(err)));
    }
  };

  const openCreateModal = () => {
    setEditingId(null);
    form.resetFields();
    form.setFieldsValue({ command_type: 'todo', enabled: true });
    setModalVisible(true);
  };

  const openEditModal = (cmd: WorkspaceSlashCommand) => {
    setEditingId(cmd.id);
    form.setFieldsValue({
      slash_command: cmd.slash_command,
      command_type: cmd.command_type,
      todo_id: cmd.command_type === 'todo' ? cmd.todo_id : undefined,
      loop_id: cmd.command_type === 'loop' ? cmd.loop_id : undefined,
      enabled: cmd.enabled,
    });
    setModalVisible(true);
  };

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      const params: db.CreateWorkspaceSlashCommandParams = {
        slash_command: values.slash_command,
        command_type: values.command_type,
        todo_id: values.command_type === 'todo' ? values.todo_id : 0,
        // command_type === 'todo' 时用 0 告诉后端清空 loop_id；'loop' 时传具体值
        loop_id: values.command_type === 'loop' ? values.loop_id : 0,
        enabled: values.enabled,
      };
      if (editingId) {
        await db.updateWorkspaceSlashCommand(workspaceId, editingId, params);
        message.success('更新成功');
      } else {
        await db.createWorkspaceSlashCommand(workspaceId, params);
        message.success('创建成功');
      }
      setModalVisible(false);
      loadCommands();
      onChanged?.();
    } catch (err: any) {
      if (!err?.errorFields) {
        message.error('操作失败: ' + (err?.message || String(err)));
      }
    }
  };

  const columns: ColumnsType<WorkspaceSlashCommand> = [
    {
      title: '命令',
      dataIndex: 'slash_command',
      key: 'slash_command',
      render: (cmd: string) => <code style={{ fontSize: 14 }}>{cmd}</code>,
    },
    {
      title: '类型',
      dataIndex: 'command_type',
      key: 'command_type',
      width: 80,
      render: (type: 'todo' | 'loop') => type === 'todo' ? 'Todo' : '环路',
    },
    {
      title: '绑定目标',
      key: 'target',
      render: (_: any, cmd: WorkspaceSlashCommand) => {
        if (cmd.command_type === 'todo') {
          const todo = todos.find(t => t.id === cmd.todo_id);
          return todo ? todo.title : `Todo #${cmd.todo_id}`;
        } else {
          const loop = loops.find(l => l.id === cmd.loop_id);
          return loop ? loop.name : `环路 #${cmd.loop_id}`;
        }
      },
    },
    {
      title: '启用',
      dataIndex: 'enabled',
      key: 'enabled',
      width: 80,
      render: (enabled: boolean, cmd: WorkspaceSlashCommand) => (
        <Switch checked={enabled} onChange={(v) => handleToggle(cmd, v)} />
      ),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      key: 'updated_at',
      render: (t: string) => t ? new Date(t).toLocaleString() : '-',
    },
    ...(showActions ? [{
      title: '操作',
      key: 'actions',
      width: 120,
      render: (_: any, cmd: WorkspaceSlashCommand) => (
        <Space>
          <Button
            type="text"
            size="small"
            icon={<EditOutlined />}
            onClick={() => openEditModal(cmd)}
          />
          <Popconfirm
            title="确定删除此斜杠命令？"
            onConfirm={() => handleDelete(cmd.id)}
            okText="删除"
            cancelText="取消"
          >
            <Button type="text" size="small" icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    }] : []),
  ];

  return (
    <div>
      <div style={{ marginBottom: 16, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span style={{ color: '#666' }}>斜杠命令格式: <code>/命令名称</code>，自动匹配消息前缀</span>
        <Button type="primary" icon={<PlusOutlined />} onClick={openCreateModal}>
          添加斜杠命令
        </Button>
      </div>

      <Table
        columns={columns}
        dataSource={commands}
        rowKey="id"
        loading={loading}
        pagination={false}
        locale={{ emptyText: '暂无斜杠命令，点击添加' }}
      />

      <Modal
        title={editingId ? '编辑斜杠命令' : '添加斜杠命令'}
        open={modalVisible}
        onOk={handleSubmit}
        onCancel={() => setModalVisible(false)}
        okText={editingId ? '保存' : '创建'}
        cancelText="取消"
        width={560}
      >
        <Form form={form} layout="vertical" style={{ marginTop: 16 }}>
          <Form.Item
            name="slash_command"
            label="斜杠命令"
            rules={[
              { required: true, message: '请输入斜杠命令' },
              { pattern: /^\//, message: '命令必须以 / 开头' },
            ]}
          >
            <Input placeholder="/todo" />
          </Form.Item>

          <Form.Item
            name="command_type"
            label="命令类型"
            rules={[{ required: true, message: '请选择命令类型' }]}
          >
            <Radio.Group>
              <Radio.Button value="todo">Todo</Radio.Button>
              <Radio.Button value="loop">环路</Radio.Button>
            </Radio.Group>
          </Form.Item>

          {commandType === 'todo' && (
            <Form.Item
              name="todo_id"
              label="绑定 Todo"
              rules={[{ required: true, message: '请选择绑定的 Todo' }]}
            >
              <Select showSearch placeholder="选择 Todo" filterOption={(input, option) =>
                (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
              }>
                {todos.map(todo => (
                  <Select.Option key={todo.id} value={todo.id} label={todo.title}>
                    {todo.title}
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          )}

          {commandType === 'loop' && (
            <Form.Item
              name="loop_id"
              label="绑定环路"
              rules={[{ required: true, message: '请选择绑定的环路' }]}
            >
              <Select showSearch placeholder="选择环路" filterOption={(input, option) =>
                (option?.label as string)?.toLowerCase().includes(input.toLowerCase())
              }>
                {loops.map(loop => (
                  <Select.Option key={loop.id} value={loop.id} label={loop.name}>
                    {loop.name}
                  </Select.Option>
                ))}
              </Select>
            </Form.Item>
          )}

          <Form.Item name="enabled" label="启用状态" valuePropName="checked" initialValue={true}>
            <Switch />
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
