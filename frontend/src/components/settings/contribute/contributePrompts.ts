/**
 * 资源贡献（提交 PR）的 ActionButton Prompt 模板（工艺 / 事项模板 / 技能）。
 *
 * 与 experts/contributePrompt.ts（专家版）同源设计：PAT 不内联、owner/repo 固定、
 * 资源提交到远端仓库对应子目录。动态值全部走 {{...}} 占位符，由 ShareToRepoButton 的
 * params 注入（含 onPrepare 异步产出的键，如事项模板导出后的文件路径）。
 *
 * 提示词正文用数组拼接而非单个模板字符串：正文里大量代码引用反引号（如 `cat ...`），
 * 塞进模板字符串需要层层转义，可读性差且易错。
 */

/**
 * 组装公共提交模板。
 *
 * 占位符：{{resource_name}} / {{version}} / {{resource_dir}} / {{remote_path}}。
 * @param titlePrefix PR 标题前缀（工艺/事项模板/技能）
 * @param isDirectory true=遍历目录（技能），false=读取单个文件（工艺/事项模板）
 */
function buildBasePrompt(titlePrefix: string, isDirectory: boolean): string {
  // 单文件与目录遍历的读取指令不同：单文件只读一个文件，目录遍历要递归收集并跳过同步元数据
  const fileMode = isDirectory
    ? '先展开为绝对路径，再遍历该目录，收集每个文件的「相对该目录的路径」与内容；跳过 .downloaded_at、.clawhub、.git 三类同步元数据。'
    : '先展开为绝对路径，读取该文件的内容（UTF-8 文本）。';
  return [
    `请把本地${titlePrefix}「{{resource_name}}」{{version}}打包提交到 GitCode 官方仓库，作为一个 PR 供维护者审核。`,
    '',
    '## 关键信息',
    '- 资源目录：{{resource_dir}}（~ 表示当前用户家目录，执行前先展开为绝对路径）',
    '- 官方仓库：weibaohui/ntd-resource（GitCode，API base = https://api.gitcode.com）',
    '- PAT 位置：~/.ntd/contribution_pat.json（JSON 结构：{"pat":"..."}）。只读取其中的 pat 字段。',
    '',
    '## 执行步骤（严格按顺序）',
    '1. 读取 PAT：执行 `cat ~/.ntd/contribution_pat.json`，取出 pat 字段。注意 pat 是敏感凭据，读取后不要把它的明文打印到输出、日志或最终结果里。',
    `2. ${fileMode}`,
    '3. 把第 1 步读到的 pat 作为 HTTP 认证令牌，附加到下面每个 GitCode API 请求的认证头里（bearer 认证方式），不要写成占位符：',
    '   a. 验证用户：`GET https://api.gitcode.com/api/v5/user`，拿到返回的 login 字段——这是 PAT 真实所属的账号，后续所有 URL 里的 {owner} 一律用它。',
    '   b. fork：`POST https://api.gitcode.com/api/v5/repos/weibaohui/ntd-resource/forks`；若返回 409/422 表示已 fork，视为成功。',
    '   c. 建分支：`POST https://api.gitcode.com/api/v5/repos/{步骤 a 的 login}/ntd-resource/branches`，JSON body 为 {"branch_name":"contrib/{{resource_name}}-<unix 时间戳>","refs":"main"}。',
    '   d. 写文件：`POST https://api.gitcode.com/api/v5/repos/{步骤 a 的 login}/ntd-resource/contents/{{remote_path}}`。**必须**用这个远端路径，不能写到仓库根目录。表单字段 content=<文件字节的 base64>、message="贡献' + titlePrefix + ' {{resource_name}} {{version}}"+（如有）、branch=<步骤 c 的分支名>。',
    '   e. 创建 PR：`POST https://api.gitcode.com/api/v5/repos/weibaohui/ntd-resource/pulls`，JSON body 为 {"title":"[' + titlePrefix + '] {{resource_name}} {{version}}","body":"<资源元信息表格 + 文件清单>","head":"{步骤 a 的 login}:{branch}","base":"main"}。',
    '4. 完成后，最终输出 PR 的网页链接（响应里的 web_url 字段）。',
    '',
    '## 注意',
    '- PAT 是敏感凭据，任何输出里都不要回显 PAT 明文。',
    '- 如果任一步骤失败，先检查错误信息，不要盲目重试；若 PAT 失效，提示用户重新填写。',
  ].join('\n');
}

/** 工艺贡献模板：单文件提交到远端 processes/（分类子目录由 remote_path 决定） */
export function buildProcessContributePrompt(): string {
  return buildBasePrompt('工艺', false);
}

/** 事项模板贡献模板：单文件提交到远端 todos/（文件由后端导出生成） */
export function buildTodoContributePrompt(): string {
  return buildBasePrompt('事项模板', false);
}

/** 技能贡献模板：目录提交到远端 skills/{source}/{skill}/ */
export function buildSkillContributePrompt(): string {
  return buildBasePrompt('技能', true);
}