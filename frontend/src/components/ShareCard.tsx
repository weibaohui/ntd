import { Card, App } from 'antd';
import { ShareAltOutlined } from '@ant-design/icons';
import { CopyButton } from '@/components/CopyButton';

const getSharePrompt = () => {
  return `全局安装 ntd：npm install -g @weibaohui/ntd，然后执行 ntd daemon install && ntd daemon start，可选安装 skills：ntd skills install，访问 8088 端口`;
};

export function ShareCard() {
  const { message } = App.useApp();

  return (
    <Card
      title={
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ShareAltOutlined />
          <span>分享给朋友</span>
        </div>
      }
      className="dashboard-card"
      style={{ borderRadius: 12 }}
      bodyStyle={{ padding: '16px 20px' }}
    >
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ fontSize: 13, color: 'var(--color-text-secondary)' }}>
          将下方提示词复制发给 AI 助手，即可自动完成安装：
        </div>
        <div
          style={{
            background: 'var(--color-fill-quaternary)',
            borderRadius: 8,
            padding: '12px 16px',
            fontFamily: 'monospace',
            fontSize: 13,
            lineHeight: 1.8,
            whiteSpace: 'pre-wrap',
            position: 'relative',
          }}
        >
          {getSharePrompt()}
        </div>
        <CopyButton
          type="primary"
          text={getSharePrompt()}
          onCopy={() => message.success('安装提示词已复制到剪贴板')}
          style={{ alignSelf: 'flex-end' }}
        >
          复制提示词
        </CopyButton>
      </div>
    </Card>
  );
}
