/**
 * 专家贡献（提交 PR）的 ActionButton Prompt 模板。
 *
 * 设计取舍：
 * - PAT 不内联进 prompt（避免明文 PAT 落库到 todo.prompt / 执行记录），
 *   而是指示 AI 在执行时从 ~/.ntd/contribution_pat.json 读取。
 * - 官方仓库 owner/repo 固定为 weibaohui/ntd-resource，与 bundled 同步源一致。
 * - 仓库内专家统一存放在 experts/<专家名>/ 下（与 bundled 同步源目录结构一致），
 *   所以上传路径必须以 experts/{{expert_name}}/ 为前缀，不能直接写到仓库根目录。
 * - 占位符 {{expert_name}} / {{version}} / {{expert_dir}} 由 ActionButton 的 params 注入。
 * - expert_dir 由调用方转成 ~/.ntd/... 家目录相对路径（toHomePath），
 *   提示词要求 AI 先展开 ~ 再遍历，既避免暴露家目录用户名，又不依赖 AI 猜路径。
 */

export const CONTRIBUTE_ACTION_TYPE = 'expert_contribute';

/**
 * 组装贡献提交 prompt（模板，占位符由 ActionButton 替换）。
 */
export function buildContributePrompt(): string {
  return `请把本地专家「{{expert_name}}」（版本 {{version}}）打包提交到 GitCode 官方仓库，作为一个 PR 供维护者审核。

## 关键信息
- 专家目录：{{expert_dir}}（~ 表示当前用户家目录，执行前先展开为绝对路径）
- 官方仓库：weibaohui/ntd-resource（GitCode，API base = https://api.gitcode.com）
- PAT 位置：~/.ntd/contribution_pat.json（JSON 结构：{"pat":"..."}）。只读取其中的 pat 字段。

## 执行步骤（严格按顺序）
1. 读取 PAT：执行 \`cat ~/.ntd/contribution_pat.json\`，取出 pat 字段。注意 pat 是敏感凭据，读取后不要把它的明文打印到输出、日志或最终结果里。
2. 先把 {{expert_dir}} 中的 ~ 展开为当前用户家目录的绝对路径，再遍历该目录，收集每个文件的「相对该目录的路径」与内容；跳过 .downloaded_at、.clawhub、.git 三类同步元数据。
3. 把第 1 步读到的 pat 作为 HTTP 认证令牌，附加到下面每个 GitCode API 请求的认证头里（bearer 认证方式），不要写成占位符：
   a. 验证用户：\`GET https://api.gitcode.com/api/v5/user\`，拿到返回的 login 字段——这是 PAT 真实所属的账号，后续所有 URL 里的 {owner} 一律用它。
   b. fork：\`POST https://api.gitcode.com/api/v5/repos/weibaohui/ntd-resource/forks\`；若返回 409/422 表示已 fork，视为成功。
   c. 建分支：\`POST https://api.gitcode.com/api/v5/repos/{步骤 a 的 login}/ntd-resource/branches\`，JSON body 为 {"branch_name":"contrib/{{expert_name}}-<unix 时间戳>","refs":"main"}。
   d. 逐文件写入：\`POST https://api.gitcode.com/api/v5/repos/{步骤 a 的 login}/ntd-resource/contents/experts/{{expert_name}}/{第 2 步的相对路径}\`。仓库中专家统一存放在 experts/{{expert_name}}/ 下（与 bundled 同步源目录结构一致），**必须**带这个前缀，不能写到仓库根目录。表单字段 content=<文件字节的 base64>、message="贡献专家 {{expert_name}} v{{version}}"、branch=<步骤 c 的分支名>。
   e. 创建 PR：\`POST https://api.gitcode.com/api/v5/repos/weibaohui/ntd-resource/pulls\`，JSON body 为 {"title":"[专家贡献] {{expert_name}} v{{version}}","body":"<专家元信息表格 + 文件清单>","head":"{步骤 a 的 login}:{branch}","base":"main"}。
4. 完成后，最终输出 PR 的网页链接（响应里的 web_url 字段）。

## 注意
- PAT 是敏感凭据，任何输出里都不要回显 PAT 明文。
- 如果任一步骤失败，先检查错误信息，不要盲目重试；若 PAT 失效，提示用户重新填写。`;
}
