// 第三方授权面板：统一管理各外部平台的凭据配置。
// 当前仅含 GitCode（专家「分享」提交 PR 需要 PAT）；后续平台在此追加子 Tab。
// PAT 管理从「分享」按钮旁收口到这里：分享区不再放清除入口，避免误触。

import { useCallback, useEffect, useState } from 'react';
import { Alert, App, Button, Form, Input, Popconfirm, Space, Tabs, Tag, Typography } from 'antd';
import { CheckCircleOutlined } from '@ant-design/icons';
import {
  getContributionAuthStatus,
  logoutContribution,
  saveContributionPat,
  verifyContributionPat,
} from '@/utils/database/contribution';

/** 设置页「第三方授权」Tab 的 key：SettingsPage 用它注册 tab，ContributeButton 用它跳转。 */
export const THIRD_PARTY_SETTINGS_TAB = 'thirdParty';

/**
 * 第三方授权面板入口：内嵌子 Tabs，第一个为 GitCode。
 * 只做路由分发，具体平台表单由子组件负责，便于后续按平台扩展。
 */
export function ThirdPartyPanel() {
  return (
    <Tabs
      items={[
        {
          key: 'gitcode',
          label: 'GitCode',
          children: <GitCodePatTab />,
        },
      ]}
    />
  );
}

/**
 * GitCode PAT 管理表单：填写 / 保存 / 验证 / 清空。
 * 保存走后端验证接口（调 GitCode /user 确认有效后才落盘），
 * 「验证」读取已保存 PAT 获取用户名，证明令牌当前可用；
 * 清空是破坏性操作，用 Popconfirm 二次确认，未配置时禁用。
 */
function GitCodePatTab() {
  const { message } = App.useApp();
  const [form] = Form.useForm();
  // 配置态三态：null=查询中，true=已配置，false=未配置；决定状态标签与清空按钮可用性。
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [saving, setSaving] = useState(false);
  const [clearing, setClearing] = useState(false);
  // 验证态：null=未验证过，'verifying'=验证中，其余为已成功验证的账号信息（含用户名）。
  // 与 configured 分开管理：PAT 保存/清空后应清空旧验证结果，避免展示过期身份。
  const [verifyResult, setVerifyResult] = useState<{ username: string; name: string } | null>(null);
  const [verifying, setVerifying] = useState(false);

  // 拉取当前配置态：进入面板即查一次，保存/清空成功后刷新。
  const loadStatus = useCallback(async () => {
    try {
      const status = await getContributionAuthStatus();
      setConfigured(status.configured);
    } catch (err: any) {
      message.error('查询 PAT 配置态失败: ' + (err?.message || String(err)));
    }
  }, [message]);

  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  // 保存 PAT：先过表单必填校验，再调后端验证并持久化；成功清空输入并刷新状态。
  const handleSave = async () => {
    let values: { pat?: string };
    try {
      values = await form.validateFields();
    } catch (err: any) {
      // 表单校验失败（如为空）由 Form.Item 展示错误，无需额外提示。
      if (err?.errorFields) return;
      throw err;
    }
    setSaving(true);
    try {
      await saveContributionPat(values.pat!.trim());
      message.success('PAT 保存成功');
      form.resetFields();
      await loadStatus();
    } catch (err: any) {
      message.error('PAT 保存失败: ' + (err?.message || String(err)));
    } finally {
      setSaving(false);
    }
  };

  // 验证已保存的 PAT：后端读本地 PAT 调 GitCode /user，返回用户名即证明令牌可用。
  // 失败不区分「无效/网络」在展示层细化——错误信息由后端按原因分类返回，直接透出。
  const handleVerify = async () => {
    setVerifying(true);
    setVerifyResult(null);
    try {
      const result = await verifyContributionPat();
      setVerifyResult(result);
    } catch (err: any) {
      message.error('PAT 验证失败: ' + (err?.message || String(err)));
    } finally {
      setVerifying(false);
    }
  };

  // 清空 PAT：调后端清除本地文件；成功后刷新配置态并清空旧验证结果。
  const handleClear = async () => {
    setClearing(true);
    try {
      await logoutContribution();
      message.success('已清除 PAT');
      setVerifyResult(null);
      await loadStatus();
    } catch (err: any) {
      message.error('清除 PAT 失败: ' + (err?.message || String(err)));
    } finally {
      setClearing(false);
    }
  };

  return (
    <div style={{ maxWidth: 560 }}>
      {/* 简短的用途标题即可，细节留在下方辅助文案里，避免面板顶部堆字。 */}
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 16 }}
        message="GitCode 个人访问令牌（PAT）"
      />
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <div>
          当前状态：
          {configured === null ? (
            '查询中...'
          ) : configured ? (
            <Tag color="green">已配置</Tag>
          ) : (
            <Tag>未配置</Tag>
          )}
          {/* 验证按钮：仅已配置态有意义（未配置时没有可验证的 PAT）。
              验证结果以行内 Tag 呈现，让「PAT 当前可用 + 归属账号」一眼可辨。 */}
          {configured === true && (
            <>
              <Button
                size="small"
                icon={<CheckCircleOutlined />}
                loading={verifying}
                onClick={handleVerify}
                style={{ marginLeft: 12 }}
              >
                验证
              </Button>
              {verifyResult && !verifying && (
                <Tag color="green" style={{ marginLeft: 8 }}>
                  验证通过：@{verifyResult.username}
                  {verifyResult.name && verifyResult.name !== verifyResult.username
                    ? `（${verifyResult.name}）`
                    : ''}
                </Tag>
              )}
            </>
          )}
        </div>

        <Form form={form} layout="vertical" style={{ maxWidth: 480 }}>
          <Form.Item
            name="pat"
            label="Personal Access Token"
            rules={[{ required: true, message: '请输入 GitCode PAT' }]}
          >
            {/* 配置态决定输入可用性：已配置 → 禁用（防止无感知覆盖，需先清空才能换新）；
                未配置 → 可输入。占位符随状态变化，让「有值/无值」一眼可辨。 */}
            <Input.Password
              placeholder={configured === true ? '已配置，如需更换请先清空' : '粘贴 GitCode PAT'}
              disabled={configured === true}
              autoComplete="new-password"
            />
          </Form.Item>
        </Form>

        <Space>
          {/* 按钮按配置态互斥出现：已配置只保留「清空」（保存无意义），
              未配置才显示「保存」（清空无对象）。加载态（null）暂不渲染，避免闪烁。 */}
          {configured === true ? (
            <Popconfirm
              title="确定清除已保存的 PAT？"
              description="清除后，分享专家前需重新在设置中填写。"
              okText="清除"
              cancelText="取消"
              okButtonProps={{ danger: true }}
              onConfirm={handleClear}
            >
              <Button danger loading={clearing}>
                清空
              </Button>
            </Popconfirm>
          ) : configured === false ? (
            <Button type="primary" loading={saving} onClick={handleSave}>
              保存
            </Button>
          ) : null}
        </Space>

        <Typography.Text type="secondary" style={{ fontSize: 12 }}>
          生成 PAT 请前往 GitCode「个人设置 → 访问令牌」。创建 PR 的完整链路（fork、建分支、写文件、建 PR）需要仓库读写与 PR 相关权限，请务必勾选相应权限后再保存，否则 AI 执行时会因权限不足失败。保存后可点击「验证」确认令牌当前可用，并查看其归属账号。
        </Typography.Text>
      </Space>
    </div>
  );
}
