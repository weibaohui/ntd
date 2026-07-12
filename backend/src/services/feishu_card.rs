//! Feishu Interactive Card 构建器与渲染器
//!
//! 参考 cc-connect 的 `core/card.go` 和 `platform/feishu/card.go` 设计，
//! 实现平台无关的卡片抽象，再翻译为飞书 Interactive Card v1 JSON。
//!
//! 核心概念：
//! - `Card` — 卡片结构，包含可选 Header 和 Elements 列表
//! - `CardElement` — 可变元素类型：Markdown、Divider、Actions、ListItem、Note、Select
//! - `CardButton` — 按钮，包含显示文本、类型、回调值
//! - Builder 模式 — 流畅 API，逐步构建卡片

use serde_json::Value;

// ============================================================================
// Card 数据结构
// ============================================================================

/// 卡片 Header（彩色标题栏）
#[derive(Debug, Clone)]
pub struct CardHeader {
    pub title: String,
    pub color: String, // blue, green, red, orange, purple, grey, turquoise, violet, indigo, wathet, yellow, carmine
}

/// Markdown 文本元素
#[derive(Debug, Clone)]
pub struct CardMarkdown {
    pub content: String,
}

/// 分隔线元素
#[derive(Debug, Clone)]
pub struct CardDivider {}

/// 按钮行布局
#[derive(Debug, Clone, PartialEq)]
pub enum CardActionLayout {
    Row,           // 普通行布局
    EqualColumns,  // 等宽列布局（用于 Tab 按钮，2-per-row）
}

/// 按钮行元素
#[derive(Debug, Clone)]
pub struct CardActions {
    pub buttons: Vec<CardButton>,
    pub layout: CardActionLayout,
}

/// 单个按钮
#[derive(Debug, Clone)]
pub struct CardButton {
    pub text: String,              // 显示文本
    pub button_type: String,       // "primary" | "default" | "danger"
    pub value: String,             // 回调值，如 "nav:/help session"
    pub extra: std::collections::HashMap<String, String>, // 额外键值对
}

/// 列表项：左侧描述 + 右侧按钮
#[derive(Debug, Clone)]
pub struct CardListItem {
    pub text: String,              // 左侧描述文本
    pub btn_text: String,          // 按钮文本
    pub btn_type: String,          // 按钮类型
    pub btn_value: String,         // 按钮回调值
    pub extra: std::collections::HashMap<String, String>,
}

/// 下拉选择器
#[derive(Debug, Clone)]
pub struct CardSelectOption {
    pub text: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct CardSelect {
    pub placeholder: String,
    pub options: Vec<CardSelectOption>,
    pub init_value: String,
    /// 选中后随事件回传的 action value（路由用），如 "act:/push result_only"。
    /// select_static 的 value 字段会原样回传，handle_card_callback 据此按 act:/ 前缀分发。
    pub action: String,
}

/// 底部备注
#[derive(Debug, Clone)]
pub struct CardNote {
    pub text: String,
}

/// 卡片元素（枚举）
#[derive(Debug, Clone)]
pub enum CardElement {
    Markdown(CardMarkdown),
    Divider(CardDivider),
    Actions(CardActions),
    ListItem(CardListItem),
    Note(CardNote),
    Select(CardSelect),
}

/// 完整卡片
#[derive(Debug, Clone)]
pub struct Card {
    pub header: Option<CardHeader>,
    pub elements: Vec<CardElement>,
}

// ============================================================================
// Builder API
// ============================================================================

/// 卡片构建器（流畅 API）
pub struct CardBuilder {
    card: Card,
}

impl CardBuilder {
    pub fn new() -> Self {
        Self {
            card: Card {
                header: None,
                elements: Vec::new(),
            },
        }
    }

    /// 设置标题
    pub fn title(mut self, title: &str, color: &str) -> Self {
        self.card.header = Some(CardHeader {
            title: title.to_string(),
            color: color.to_string(),
        });
        self
    }

    /// 添加 Markdown 文本
    pub fn markdown(mut self, content: &str) -> Self {
        if !content.is_empty() {
            self.card.elements.push(CardElement::Markdown(CardMarkdown {
                content: content.to_string(),
            }));
        }
        self
    }

    /// 添加分隔线
    pub fn divider(mut self) -> Self {
        self.card.elements.push(CardElement::Divider(CardDivider {}));
        self
    }

    /// 添加按钮行（普通布局）
    pub fn buttons(mut self, buttons: Vec<CardButton>) -> Self {
        if !buttons.is_empty() {
            self.card.elements.push(CardElement::Actions(CardActions {
                buttons,
                layout: CardActionLayout::Row,
            }));
        }
        self
    }

    /// 添加按钮行（等宽列布局，用于 Tab 按钮）
    pub fn buttons_equal(mut self, buttons: Vec<CardButton>) -> Self {
        if !buttons.is_empty() {
            self.card.elements.push(CardElement::Actions(CardActions {
                buttons,
                layout: CardActionLayout::EqualColumns,
            }));
        }
        self
    }

    /// 添加列表项（描述 + 按钮）
    pub fn list_item(mut self, text: &str, btn_text: &str, btn_value: &str) -> Self {
        self.card.elements.push(CardElement::ListItem(CardListItem {
            text: text.to_string(),
            btn_text: btn_text.to_string(),
            btn_type: "default".to_string(),
            btn_value: btn_value.to_string(),
            extra: std::collections::HashMap::new(),
        }));
        self
    }

    /// 添加列表项（指定按钮类型）
    pub fn list_item_btn(mut self, text: &str, btn_text: &str, btn_type: &str, btn_value: &str) -> Self {
        self.card.elements.push(CardElement::ListItem(CardListItem {
            text: text.to_string(),
            btn_text: btn_text.to_string(),
            btn_type: btn_type.to_string(),
            btn_value: btn_value.to_string(),
            extra: std::collections::HashMap::new(),
        }));
        self
    }

    /// 添加备注
    pub fn note(mut self, text: &str) -> Self {
        if !text.is_empty() {
            self.card.elements.push(CardElement::Note(CardNote {
                text: text.to_string(),
            }));
        }
        self
    }

    /// 添加下拉选择器（如推送级别三选一）。
    /// select.action 是选中回调的路由值；select_static 的 value 会原样回传给 handle_card_callback。
    pub fn select(mut self, select: CardSelect) -> Self {
        self.card.elements.push(CardElement::Select(select));
        self
    }

    /// 构建卡片
    pub fn build(self) -> Card {
        self.card
    }
}

impl Default for CardBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 按钮便捷构造函数
// ============================================================================

impl CardButton {
    /// 创建普通按钮
    pub fn new(text: &str, button_type: &str, value: &str) -> Self {
        Self {
            text: text.to_string(),
            button_type: button_type.to_string(),
            value: value.to_string(),
            extra: std::collections::HashMap::new(),
        }
    }

    /// 创建 Primary 按钮
    pub fn primary(text: &str, value: &str) -> Self {
        Self::new(text, "primary", value)
    }

    /// 创建 Default 按钮
    pub fn default_btn(text: &str, value: &str) -> Self {
        Self::new(text, "default", value)
    }

    /// 创建 Danger 按钮
    pub fn danger(text: &str, value: &str) -> Self {
        Self::new(text, "danger", value)
    }
}

// ============================================================================
// Feishu Interactive Card 渲染器
// ============================================================================

/// 渲染卡片为飞书 Interactive Card v1 JSON
pub fn render_card(card: &Card, session_key: &str) -> String {
    let map = render_card_map(card, session_key);
    serde_json::to_string(&map).unwrap_or_else(|_| r#"{"config":{"wide_screen_mode":true},"elements":[]}"#.to_string())
}

/// 渲染卡片为 serde_json::Value
/// 使用飞书卡片 JSON 2.0 格式：{schema: "2.0", body: {elements}, header}
fn render_card_map(card: &Card, session_key: &str) -> Value {
    let mut result = serde_json::json!({
        "schema": "2.0",
        "body": {
            "elements": []
        }
    });

    // Header（JSON 2.0 格式）
    if let Some(ref header) = card.header {
        let color = if header.color.is_empty() { "blue" } else { &header.color };
        result["header"] = serde_json::json!({
            "title": {
                "tag": "plain_text",
                "content": header.title
            },
            "template": color
        });
    }

    // Elements（放在 body.elements 中）
    let elements = render_elements(&card.elements, session_key);
    if elements.is_empty() {
        result["body"]["elements"] = serde_json::json!([{"tag": "markdown", "content": " "}]);
    } else {
        result["body"]["elements"] = Value::Array(elements);
    }

    result
}

/// 渲染所有元素
fn render_elements(elements: &[CardElement], session_key: &str) -> Vec<Value> {
    let mut result = Vec::new();
    for elem in elements {
        match elem {
            CardElement::Markdown(md) => {
                result.push(serde_json::json!({
                    "tag": "markdown",
                    "content": md.content
                }));
            }
            CardElement::Divider(_) => {
                result.push(serde_json::json!({"tag": "hr"}));
            }
            CardElement::Actions(actions) => {
                let rendered = render_actions(actions, session_key);
                result.extend(rendered);
            }
            CardElement::ListItem(item) => {
                result.push(render_list_item(item, session_key));
            }
            CardElement::Note(note) => {
                result.push(serde_json::json!({
                    "tag": "markdown",
                    "content": note.text
                }));
            }
            CardElement::Select(select) => {
                result.push(render_select(select, session_key));
            }
        }
    }
    result
}

/// 渲染按钮行
fn render_actions(actions: &CardActions, session_key: &str) -> Vec<Value> {
    if actions.buttons.is_empty() {
        return Vec::new();
    }
    match actions.layout {
        CardActionLayout::EqualColumns => render_equal_columns(&actions.buttons, session_key),
        CardActionLayout::Row => render_row_buttons(&actions.buttons, session_key),
    }
}

/// 等宽列布局：每个按钮一个 column，2个按钮时用 bisect。
fn render_equal_columns(buttons: &[CardButton], session_key: &str) -> Vec<Value> {
    let mut columns = Vec::new();
    for btn in buttons {
        columns.push(serde_json::json!({
            "tag": "column",
            "width": "weighted",
            "weight": 1,
            "vertical_align": "center",
            "horizontal_align": "center",
            "elements": [render_button(btn, session_key, true)]
        }));
    }
    let mut column_set = serde_json::json!({
        "tag": "column_set",
        "columns": columns
    });
    // 2个按钮时使用 bisect 布局
    if buttons.len() == 2 {
        column_set["flex_mode"] = serde_json::json!("bisect");
    }
    vec![column_set]
}

/// 行布局：按钮行用 column_set（每个按钮一列、等宽），不强制居中对齐。
fn render_row_buttons(buttons: &[CardButton], session_key: &str) -> Vec<Value> {
    // 飞书卡片 schema V2 不支持 tag:action，按钮行改用 column_set
    let columns: Vec<Value> = buttons
        .iter()
        .map(|btn| serde_json::json!({
            "tag": "column",
            "width": "weighted",
            "weight": 1,
            "elements": [render_button(btn, session_key, false)]
        }))
        .collect();
    vec![serde_json::json!({
        "tag": "column_set",
        "flex_mode": "none",
        "columns": columns,
    })]
}

/// 渲染单个按钮
fn render_button(btn: &CardButton, session_key: &str, fill_width: bool) -> Value {
    let btn_type = if btn.button_type.is_empty() { "default" } else { &btn.button_type };

    let mut value_map = serde_json::json!({
        "action": btn.value
    });

    // 注入 session_key
    if !session_key.is_empty() {
        value_map["session_key"] = serde_json::json!(session_key);
    }

    // 注入 extra
    for (k, v) in &btn.extra {
        value_map[k] = serde_json::json!(v);
    }

    let mut obj = serde_json::json!({
        "tag": "button",
        "text": {
            "tag": "plain_text",
            "content": btn.text
        },
        "type": btn_type,
        "value": value_map
    });

    if fill_width {
        obj["width"] = serde_json::json!("fill");
    }

    obj
}

/// 渲染列表项（左侧描述 + 右侧按钮）
fn render_list_item(item: &CardListItem, session_key: &str) -> Value {
    let btn_type = if item.btn_type.is_empty() { "default" } else { &item.btn_type };

    let mut value_map = serde_json::json!({
        "action": item.btn_value
    });

    if !session_key.is_empty() {
        value_map["session_key"] = serde_json::json!(session_key);
    }

    for (k, v) in &item.extra {
        value_map[k] = serde_json::json!(v);
    }

    serde_json::json!({
        "tag": "column_set",
        "flex_mode": "none",
        "columns": [
            {
                "tag": "column",
                "width": "weighted",
                "weight": 5,
                "vertical_align": "center",
                "elements": [{
                    "tag": "markdown",
                    "content": item.text
                }]
            },
            {
                "tag": "column",
                "width": "auto",
                "vertical_align": "center",
                "elements": [{
                    "tag": "button",
                    "text": {
                        "tag": "plain_text",
                        "content": item.btn_text
                    },
                    "type": btn_type,
                    "value": value_map
                }]
            }
        ]
    })
}

/// 渲染下拉选择器
fn render_select(select: &CardSelect, session_key: &str) -> Value {
    // 生成飞书 option 对象列表
    let options = build_select_options(&select.options);
    let mut elem = serde_json::json!({
        "tag": "select_static",
        "placeholder": {
            "tag": "plain_text",
            "content": select.placeholder
        },
        "options": options
    });
    // 把 action（路由用）和 session_key 塞进 value，选中事件回传后能按 act:/ 前缀分发
    set_select_value(&mut elem, &select.action, session_key);
    // 初始选中值回显（如推送级别）
    set_initial_option(&mut elem, &select.options, &select.init_value);
    serde_json::json!({
        "tag": "action",
        "actions": [elem]
    })
}

/// 把 CardSelectOption 列表转为飞书 option 对象数组。
fn build_select_options(options: &[CardSelectOption]) -> Vec<Value> {
    options.iter().map(|opt| {
        serde_json::json!({
            "text": {"tag": "plain_text", "content": opt.text},
            "value": opt.value
        })
    }).collect()
}

/// 向 select_static 的 value 中注入 action + session_key，供回调时路由分发用。
fn set_select_value(elem: &mut Value, action: &str, session_key: &str) {
    let mut select_value = serde_json::Map::new();
    if !action.is_empty() {
        select_value.insert("action".to_string(), serde_json::json!(action));
    }
    if !session_key.is_empty() {
        select_value.insert("session_key".to_string(), serde_json::json!(session_key));
    }
    if !select_value.is_empty() {
        elem["value"] = Value::Object(select_value);
    }
}

/// 设置 select_static 的初始选中项（initial_option），与 init_value 匹配。
fn set_initial_option(elem: &mut Value, options: &[CardSelectOption], init_value: &str) {
    // 飞书 select_static 的初始选中用 initial_option（单个 option 对象），不是数组。
    if !init_value.is_empty() {
        if let Some(opt) = options.iter().find(|o| o.value == init_value) {
            elem["initial_option"] = serde_json::json!({
                "text": {"tag": "plain_text", "content": opt.text},
                "value": opt.value
            });
        }
    }
}

// ============================================================================
// 状态感知的任务控制台卡片（/help 重设计）
// 接收 listener 查出的 HelpCardState，把「当前项目/运行状态/推送级别/最近任务」
// 直接渲染进卡片，点按钮原地操作（act 执行后 patch 刷新）。
// ============================================================================

/// /help 卡片运行时状态。listener 按 agent_bot.workspace_id 查 DB（该 workspace 的
/// todos/loops/records + 推送级别 + 所有 workspace）组装，卡片层只读渲染。
#[derive(Debug, Clone, Default)]
pub struct HelpCardState {
    /// 当前 Tab：status(默认) / todo / loop / workspace
    pub current_group: String,
    /// 当前工作空间（agent_bot.workspace_id）
    pub workspace: Option<WorkspaceSummary>,
    /// 是否有运行中任务
    pub is_running: bool,
    /// 推送级别 disabled / result_only / all
    pub push_level: String,
    /// 最近任务（状态页）
    pub recent_records: Vec<RecentTaskItem>,
    /// 事项列表（事项页，按 workspace）
    pub todos: Vec<TodoItem>,
    /// 环路列表（环路页，按 workspace）
    pub loops: Vec<LoopItem>,
    /// 事项/环路分页页码（从 1 开始；Default 0 时 build 兜底为 1）
    pub page: usize,
    /// 所有工作空间（工作空间页切换用）
    pub workspaces: Vec<WorkspaceItem>,
    /// 已注册的可用执行器列表（工作空间页切换默认执行器用）。
    /// listener 从 executor_registry.list_executors() 拉出，卡片层只读渲染成按钮排。
    pub available_executors: Vec<ExecutorOption>,
}

/// 可选执行器项（工作空间页「默认执行器」按钮排）。
#[derive(Debug, Clone)]
pub struct ExecutorOption {
    /// 执行器名（ExecutorType::as_str，如 "pi" / "claudecode"）
    pub name: String,
    /// 是否为当前工作空间已配的默认执行器（primary 高亮）
    pub is_current: bool,
}

/// 当前工作空间摘要（状态页/工作空间页顶部展示）。
#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub id: i64,
    pub name: String,
    pub executor: String,
}

/// 工作空间列表项（工作空间页切换）。
#[derive(Debug, Clone)]
pub struct WorkspaceItem {
    pub id: i64,
    pub name: String,
    pub is_current: bool,
}

/// 事项列表项（事项页）。
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub id: i64,
    pub title: String,
    pub status_icon: String,
}

/// 环路列表项（环路页）。
#[derive(Debug, Clone)]
pub struct LoopItem {
    pub id: i64,
    pub name: String,
    pub status: String,
}

/// 最近任务列表项（状态页）。
#[derive(Debug, Clone)]
pub struct RecentTaskItem {
    pub status_icon: String,
    pub title: String,
    pub time_desc: String,
}

/// 历史记录列表项（历史子页）。
#[derive(Debug, Clone)]
pub struct HistoryItem {
    pub status_icon: String,
    pub title: String,
    pub trigger: String,
    pub time_desc: String,
}

/// 构建 /help 任务控制台卡片。4 个 Tab（事项/环路/工作空间/状态），默认「状态」。
pub fn build_help_console_card(state: &HelpCardState) -> Card {
    let current = if state.current_group.is_empty() { "status" } else { state.current_group.as_str() };
    let mut builder = CardBuilder::new().title("NTD 控制台", "blue");
    let tabs = help_tabs(current);
    for row in tabs.chunks(2).map(|c| c.to_vec()) {
        builder = builder.buttons_equal(row);
    }
    builder = builder.divider();
    builder = match current {
        "todo" => build_todo_page(builder, state),
        "loop" => build_loop_page(builder, state),
        "workspace" => build_workspace_page(builder, state),
        _ => build_status_page(builder, state),
    };
    builder.note("💡 直接发消息即可让 AI 执行任务 | 点按钮原地操作").build()
}

/// 4 个 Tab 按钮，当前 Tab 高亮 primary。
fn help_tabs(current: &str) -> Vec<CardButton> {
    [("status", "状态"), ("todo", "事项"), ("loop", "环路"), ("workspace", "工作空间")]
        .iter()
        .map(|(key, title)| {
            let btn_type = if *key == current { "primary" } else { "default" };
            CardButton::new(title, btn_type, &format!("nav:/help {}", key))
        })
        .collect()
}

/// 状态页（默认）：状态条 + 新会话/停止。
fn build_status_page(builder: CardBuilder, state: &HelpCardState) -> CardBuilder {
    let ws = state.workspace.as_ref().map(|w| w.name.as_str()).unwrap_or("未设置");
    let running = if state.is_running { "运行中" } else { "空闲" };
    builder
        .markdown(&format!(
            "**工作空间**：{}\n**运行状态**：{}\n**推送级别**：{}",
            ws, running, push_level_label(&state.push_level)
        ))
        .buttons(vec![
            CardButton::primary("🆕 新会话", "act:/new"),
            CardButton::default_btn("⏹ 停止", "act:/stop"),
        ])
}

/// 事项页：当前 workspace 的事项列表，每页 10 个，每项点 [执行] 触发该 todo。
fn build_todo_page(mut builder: CardBuilder, state: &HelpCardState) -> CardBuilder {
    if state.todos.is_empty() {
        return builder.markdown("_当前工作空间暂无事项_");
    }
    const PER_PAGE: usize = 10;
    let total_pages = state.todos.len().div_ceil(PER_PAGE);
    let page = state.page.clamp(1, total_pages);
    let start = (page - 1) * PER_PAGE;
    let end = (start + PER_PAGE).min(state.todos.len());
    for t in &state.todos[start..end] {
        builder = builder.list_item_btn(
            &format!("{} **{}**", t.status_icon, t.title),
            "执行", "default",
            &format!("act:/runtodo {}", t.id),
        );
    }
    builder = builder.divider();
    builder.buttons(pagination_buttons("todos", page, total_pages))
}

/// 环路页：当前 workspace 的环路列表，每页 10 个，每项点 [执行] 触发该 loop。
fn build_loop_page(mut builder: CardBuilder, state: &HelpCardState) -> CardBuilder {
    if state.loops.is_empty() {
        return builder.markdown("_当前工作空间暂无环路_");
    }
    const PER_PAGE: usize = 10;
    let total_pages = state.loops.len().div_ceil(PER_PAGE);
    let page = state.page.clamp(1, total_pages);
    let start = (page - 1) * PER_PAGE;
    let end = (start + PER_PAGE).min(state.loops.len());
    for l in &state.loops[start..end] {
        let btn_text = if l.status == "enabled" { "执行" } else { "已暂停" };
        builder = builder.list_item_btn(
            &format!("**{}**", l.name),
            btn_text, "default",
            &format!("act:/runloop {}", l.id),
        );
    }
    builder = builder.divider();
    builder.buttons(pagination_buttons("loops", page, total_pages))
}

/// 分页按钮（首页无「上一页」，末页无「下一页」）+ 返回状态。kind 为 "todos"/"loops"。
fn pagination_buttons(kind: &str, page: usize, total_pages: usize) -> Vec<CardButton> {
    let mut nav_btns = Vec::new();
    if page > 1 {
        nav_btns.push(CardButton::default_btn("← 上一页", &format!("nav:/{kind} {}", page - 1)));
    }
    nav_btns.push(CardButton::default_btn("返回状态", "nav:/help status"));
    if page < total_pages {
        nav_btns.push(CardButton::default_btn("下一页 →", &format!("nav:/{kind} {}", page + 1)));
    }
    nav_btns
}

/// 工作空间页：当前工作空间 + 列表[切换] + 默认执行器按钮排 + 推送 3 按钮 + 设为推送目标。
fn build_workspace_page(mut builder: CardBuilder, state: &HelpCardState) -> CardBuilder {
    builder = builder.markdown(&match &state.workspace {
        Some(w) => format!("**当前工作空间** {}（执行器 {}）", w.name, w.executor),
        None => "_未设置工作空间_".to_string(),
    });
    builder = builder.divider();
    for w in &state.workspaces {
        let (btn_text, btn_type) = if w.is_current { ("当前", "primary") } else { ("切换", "default") };
        builder = builder.list_item_btn(
            &format!("**{}**", w.name),
            btn_text, btn_type,
            &format!("act:/bind {}", w.id),
        );
    }
    builder = builder.divider();
    // 默认执行器选择：换行列出所有已注册执行器，当前配的 primary 高亮，点击即设为该 workspace 的默认执行器。
    builder = builder.markdown("**默认执行器**");
    if state.available_executors.is_empty() {
        builder = builder.markdown("_暂无已注册执行器_");
    } else {
        // 按每排最多 2 个按钮分组：执行器名（如 "claudecode"）较长，每排 4 个在窄屏会被飞书截断显示不全，每排 2 个留足宽度。
        for row in state.available_executors.chunks(2) {
            let btns: Vec<CardButton> = row.iter().map(|e| {
                let btn_type = if e.is_current { "primary" } else { "default" };
                CardButton::new(&e.name, btn_type, &format!("act:/setexecutor {}", e.name))
            }).collect();
            builder = builder.buttons(btns);
        }
    }
    builder = builder.divider();
    // 推送级别 3 按钮，当前级别 primary 高亮
    let level = state.push_level.as_str();
    builder = builder.buttons(vec![
        CardButton::new("关闭推送", if level == "disabled" { "primary" } else { "default" }, "act:/push disabled"),
        CardButton::new("仅结论", if level == "result_only" { "primary" } else { "default" }, "act:/push result_only"),
        CardButton::new("全部", if level == "all" { "primary" } else { "default" }, "act:/push all"),
    ]);
    builder.buttons(vec![CardButton::default_btn("📍 设为推送目标", "act:/sethome")])
}

/// 推送级别 → 中文标签。
fn push_level_label(level: &str) -> &'static str {
    match level {
        "result_only" => "仅结果",
        "all" => "全部过程",
        _ => "关闭",
    }
}

/// 历史子页：分页列出执行记录 + 上一页/下一页/返回控制台。
pub fn build_history_card(items: &[HistoryItem], page: usize, total_pages: usize) -> Card {
    let mut builder = CardBuilder::new().title("执行历史", "blue");
    builder = append_history_items(builder, items);
    builder = builder.divider();
    builder = append_history_nav(builder, page, total_pages);
    builder.note(&format!("第 {} / {} 页", page, total_pages.max(1))).build()
}

/// 追加历史记录条目；空列表给占位。
fn append_history_items(mut b: CardBuilder, items: &[HistoryItem]) -> CardBuilder {
    if items.is_empty() {
        return b.markdown("_暂无执行记录_");
    }
    for it in items {
        b = b.markdown(&format!("{} **{}** · {} · {}", it.status_icon, it.title, it.trigger, it.time_desc));
    }
    b
}

/// 历史子页的分页导航按钮（首页无「上一页」，末页无「下一页」）。
fn append_history_nav(builder: CardBuilder, page: usize, total_pages: usize) -> CardBuilder {
    let mut nav_btns = Vec::new();
    if page > 1 {
        nav_btns.push(CardButton::default_btn("← 上一页", &format!("nav:/history {}", page - 1)));
    }
    nav_btns.push(CardButton::default_btn("返回控制台", "nav:/help task"));
    if page < total_pages {
        nav_btns.push(CardButton::default_btn("下一页 →", &format!("nav:/history {}", page + 1)));
    }
    builder.buttons(nav_btns)
}

// ============================================================================
// 统一卡片消息类型 (用于替代纯文本消息)
// ============================================================================

/// 卡片状态颜色，对应飞书卡片的 header template
#[derive(Debug, Clone, Copy)]
pub enum CardMessageStatus {
    Success,  // green  - 操作成功
    Error,    // red    - 操作失败
    Warning,  // orange - 警告
    Info,     // blue   - 信息
    Loading,  // grey   - 加载中
}

/// 统一的消息卡片配置
pub struct CardMessageConfig {
    /// 消息状态，决定 header 颜色
    pub status: CardMessageStatus,
    /// 消息标题
    pub title: String,
    /// 消息内容（支持 Markdown）
    pub content: String,
    /// 底部操作按钮（可选）
    pub actions: Vec<CardButton>,
    /// 底部提示文本（可选）
    pub footer: Option<String>,
}

impl CardMessageConfig {
    /// 获取状态对应的颜色
    pub fn status_color(&self) -> &'static str {
        match self.status {
            CardMessageStatus::Success => "green",
            CardMessageStatus::Error => "red",
            CardMessageStatus::Warning => "orange",
            CardMessageStatus::Info => "blue",
            CardMessageStatus::Loading => "grey",
        }
    }

    /// 获取状态对应的图标 emoji
    pub fn status_icon(&self) -> &'static str {
        match self.status {
            CardMessageStatus::Success => "✅",
            CardMessageStatus::Error => "❌",
            CardMessageStatus::Warning => "⚠️",
            CardMessageStatus::Info => "ℹ️",
            CardMessageStatus::Loading => "⏳",
        }
    }
}

/// 统一消息卡片的流畅构建器
pub struct CardMessageBuilder {
    config: CardMessageConfig,
}

impl CardMessageBuilder {
    /// 创建新的构建器
    pub fn new(status: CardMessageStatus, title: &str, content: &str) -> Self {
        Self {
            config: CardMessageConfig {
                status,
                title: title.to_string(),
                content: content.to_string(),
                actions: Vec::new(),
                footer: None,
            },
        }
    }

    /// 添加操作按钮
    pub fn add_action(mut self, text: &str, action: &str) -> Self {
        self.config.actions.push(CardButton::default_btn(text, action));
        self
    }

    /// 添加主操作按钮（primary 样式）
    pub fn add_primary_action(mut self, text: &str, action: &str) -> Self {
        self.config.actions.push(CardButton::primary(text, action));
        self
    }

    /// 设置底部提示
    pub fn set_footer(mut self, footer: &str) -> Self {
        self.config.footer = Some(footer.to_string());
        self
    }

    /// 构建卡片
    pub fn build(&self) -> Card {
        let mut builder = CardBuilder::new();

        // 构建标题（带状态图标）
        let title = format!("{} {}", self.config.status_icon(), self.config.title);

        // 添加 Header
        builder = builder.title(&title, self.config.status_color());

        // 添加内容（Markdown 格式）
        builder = builder.markdown(&self.config.content);

        // 添加操作按钮（如果有）
        if !self.config.actions.is_empty() {
            builder = builder.divider();
            builder = builder.buttons(self.config.actions.clone());
        }

        // 添加底部提示（如果有）
        if let Some(ref footer) = self.config.footer {
            builder = builder.note(footer);
        }

        builder.build()
    }
}

/// 构建成功消息卡片
pub fn build_success_card(title: &str, content: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Success, title, content).build()
}

/// 构建错误消息卡片
pub fn build_error_card(title: &str, content: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Error, title, content).build()
}

/// 构建警告消息卡片
pub fn build_warning_card(title: &str, content: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Warning, title, content).build()
}

/// 构建信息消息卡片
pub fn build_info_card(title: &str, content: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Info, title, content).build()
}

/// 构建加载中消息卡片
pub fn build_loading_card(title: &str, content: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Loading, title, content).build()
}

/// 构建带操作的消息卡片
pub fn build_action_card(title: &str, content: &str, action_text: &str, action: &str) -> Card {
    CardMessageBuilder::new(CardMessageStatus::Info, title, content)
        .add_primary_action(action_text, action)
        .build()
}

// ============================================================================
// 富文本卡片 (Rich Card) - 参考 cc-connect 的实现
// ============================================================================

/// 步骤类型
#[derive(Debug, Clone, Copy)]
pub enum StepKind {
    /// 思考过程
    Thinking,
    /// 工具调用
    Tool,
}

/// 富文本卡片步骤
#[derive(Debug, Clone)]
pub struct RichStep {
    pub kind: StepKind,
    pub name: String,      // 步骤名称，如 "Thinking", "Bash", "Edit"
    pub summary: String,   // 摘要信息
    pub result: String,    // 执行结果
    pub status: String,    // 状态: "ok", "failed", "completed"
    pub exit_code: Option<i32>,
}

/// 富文本卡片状态
#[derive(Debug, Clone, Copy)]
pub enum RichCardStatus {
    /// 思考中 - 蓝色
    Thinking,
    ///工作中 - 蓝色
    Working,
    /// 完成 - 绿色
    Done,
    /// 错误 - 红色
    Error,
}

impl RichCardStatus {
    pub fn color(&self) -> &'static str {
        match self {
            RichCardStatus::Thinking | RichCardStatus::Working => "blue",
            RichCardStatus::Done => "green",
            RichCardStatus::Error => "red",
        }
    }

    pub fn title_prefix(&self) -> &'static str {
        match self {
            RichCardStatus::Thinking => "🤔 思考中",
            RichCardStatus::Working => "⚡工作中",
            RichCardStatus::Done => "✅ 完成",
            RichCardStatus::Error => "❌ 错误",
        }
    }
}

/// 富文本卡片构建器 - 用于显示 AI 执行过程的详细信息
pub struct RichCardBuilder {
    status: RichCardStatus,
    title: String,
    thinking_steps: Vec<RichStep>,
    tool_steps: Vec<RichStep>,
    answer: String,
    footer: String,
}

impl RichCardBuilder {
    pub fn new(status: RichCardStatus, title: &str) -> Self {
        Self {
            status,
            title: title.to_string(),
            thinking_steps: Vec::new(),
            tool_steps: Vec::new(),
            answer: String::new(),
            footer: String::new(),
        }
    }

    /// 添加思考步骤
    pub fn add_thinking(mut self, content: &str) -> Self {
        self.thinking_steps.push(RichStep {
            kind: StepKind::Thinking,
            name: "Thinking".to_string(),
            summary: content.to_string(),
            result: String::new(),
            status: "ok".to_string(),
            exit_code: None,
        });
        self
    }

    /// 添加工具调用步骤
    pub fn add_tool(mut self, name: &str, summary: &str, result: &str, status: &str) -> Self {
        self.tool_steps.push(RichStep {
            kind: StepKind::Tool,
            name: name.to_string(),
            summary: summary.to_string(),
            result: result.to_string(),
            status: status.to_string(),
            exit_code: None,
        });
        self
    }

    /// 设置最终答案
    pub fn set_answer(mut self, answer: &str) -> Self {
        self.answer = answer.to_string();
        self
    }

    /// 设置底部状态信息
    pub fn set_footer(mut self, footer: &str) -> Self {
        self.footer = footer.to_string();
        self
    }

    /// 构建富文本卡片
    pub fn build(&self) -> Card {
        let mut elements = Vec::new();

        // 添加思考 Panel（如果有）
        if !self.thinking_steps.is_empty() {
            elements.push(self.build_panel("💭 思考", &self.thinking_steps));
        }

        // 添加工具调用 Panel（如果有）
        if !self.tool_steps.is_empty() {
            elements.push(self.build_tool_panel("🔧 工具调用", &self.tool_steps));
        }

        // 添加答案（如果有）
        if !self.answer.is_empty() {
            elements.push(CardElement::Markdown(CardMarkdown {
                content: self.answer.clone(),
            }));
        }

        // 添加底部信息（如果有）
        if !self.footer.is_empty() {
            elements.push(CardElement::Divider(CardDivider {}));
            elements.push(CardElement::Note(CardNote {
                text: self.footer.clone(),
            }));
        }

        Card {
            header: Some(CardHeader {
                title: format!("{} {}", self.status.title_prefix(), self.title),
                color: self.status.color().to_string(),
            }),
            elements,
        }
    }

    /// 构建思考/工具面板（使用 column_set 模拟 collapsible panel 效果）
    fn build_panel(&self, title: &str, steps: &[RichStep]) -> CardElement {
        // 使用 markdown 模拟面板标题
        let mut content = format!("**{}**\n\n", title);
        for step in steps.iter().take(10) {
            content.push_str(&format!("• {}\n", step.summary));
        }
        if steps.len() > 10 {
            content.push_str(&format!("... 共 {} 条\n", steps.len()));
        }
        CardElement::Markdown(CardMarkdown { content })
    }

    /// 构建工具调用面板（带图标和状态）
    fn build_tool_panel(&self, title: &str, steps: &[RichStep]) -> CardElement {
        // 使用 markdown 模拟工具面板
        let mut content = format!("**{}**\n\n", title);
        for step in steps.iter().take(10) {
            let icon = match step.status.as_str() {
                "ok" | "completed" => "✅",
                "failed" => "❌",
                _ => "⚡",
            };
            content.push_str(&format!("{} **{}**: {}\n", icon, step.name, step.summary));
            if !step.result.is_empty() {
                content.push_str(&format!("  └ {}\n", step.result));
            }
        }
        if steps.len() > 10 {
            content.push_str(&format!("... 共 {} 条\n", steps.len()));
        }
        CardElement::Markdown(CardMarkdown { content })
    }
}

/// 构建富文本执行卡片
/// 用于显示 AI 执行过程中的思考、工具调用和最终答案
pub fn build_rich_card(
    status: RichCardStatus,
    title: &str,
    thinking: &[(&str, &str)],   // (content, result)
    tools: &[(&str, &str, &str, &str)], // (name, summary, result, status)
    answer: &str,
    footer: &str,
) -> Card {
    let mut builder = RichCardBuilder::new(status, title);

    for (content, _result) in thinking {
        builder = builder.add_thinking(content);
    }

    for (name, summary, result, status) in tools {
        builder = builder.add_tool(name, summary, result, status);
    }

    if !answer.is_empty() {
        builder = builder.set_answer(answer);
    }

    if !footer.is_empty() {
        builder = builder.set_footer(footer);
    }

    builder.build()
}

#[cfg(test)]
mod rich_card_tests {
    use super::*;

    #[test]
    fn test_rich_card_builder() {
        let card = RichCardBuilder::new(RichCardStatus::Working, "测试任务")
            .add_thinking("这是一个思考过程")
            .add_tool("Bash", "运行 ls 命令", "文件列表", "ok")
            .set_answer("**最终答案**")
            .set_footer("用时: 10s | Token: 500")
            .build();

        assert!(card.header.is_some());
        if let Some(ref header) = card.header {
            assert!(header.title.contains("工作中"));
        }
        assert!(!card.elements.is_empty());
    }

    #[test]
    fn test_build_rich_card() {
        let thinking = vec![("思考内容", "")];
        let tools = vec![("Bash", "ls -la", "文件列表", "ok")];
        let card = build_rich_card(
            RichCardStatus::Done,
            "完成的任务",
            &thinking,
            &tools,
            "这是最终答案",
            "用时: 5s",
        );

        assert!(card.header.is_some());
        let json = render_card(&card, "");
        assert!(json.contains("完成"));
        assert!(json.contains("思考"));
        assert!(json.contains("工具调用"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_builder_basic() {
        let card = CardBuilder::new()
            .title("Test Card", "blue")
            .markdown("Hello **world**")
            .divider()
            .buttons(vec![
                CardButton::primary("Click Me", "act:/test"),
                CardButton::default_btn("Cancel", "cmd:/cancel"),
            ])
            .note("Footer note")
            .build();

        assert!(card.header.is_some());
        // 使用 if let 避免 unwrap
        if let Some(ref header) = card.header {
            assert_eq!(header.title, "Test Card");
        }
        assert_eq!(card.elements.len(), 4); // markdown, divider, actions, note
    }

    /// select.action 非空时，它被写进 select_static 的 value 随选中事件回传；
    /// init_value 匹配的 option 渲染成 initial_option 单个对象（飞书标准格式，非数组）。
    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_render_select_action_and_initial_options() {
        let card = CardBuilder::new()
            .select(CardSelect {
                placeholder: "选择推送级别".to_string(),
                options: vec![
                    CardSelectOption { text: "关闭".to_string(), value: "act:/push disabled".to_string() },
                    CardSelectOption { text: "仅结果".to_string(), value: "act:/push result_only".to_string() },
                    CardSelectOption { text: "全部".to_string(), value: "act:/push all".to_string() },
                ],
                init_value: "act:/push result_only".to_string(),
                action: "act:/push".to_string(),
            })
            .build();

        let v = render_card_map(&card, "feishu:user1");
        // render_card_map 产物：{schema, body:{elements:[...]}, header}；select 渲染为 {tag:"action", actions:[select_static]}
        let select_static = v["body"]["elements"]
            .as_array()
            .expect("elements 数组")
            .iter()
            .find(|e| e["tag"] == "action")
            .expect("找到 action 元素")
            .get("actions")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .expect("select_static");
        assert_eq!(select_static["tag"], "select_static");
        // value 含 action（路由用）和 session_key
        assert_eq!(select_static["value"]["action"], "act:/push");
        assert_eq!(select_static["value"]["session_key"], "feishu:user1");
        // initial_option 是单个 option 对象，回显当前 result_only
        assert_eq!(select_static["initial_option"]["value"], "act:/push result_only");
    }

    /// select.action 为空（选项 2 设计：具体值在 option.value）时，value 不含 action 字段；
    /// init_value 为空时不渲染 initial_option。channel.rs 取不到 value["action"] 会 fallback 到 option。
    #[test]
    #[allow(clippy::expect_used, clippy::unwrap_used)]
    fn test_render_select_empty_action_omits_action_field() {
        let card = CardBuilder::new()
            .select(CardSelect {
                placeholder: "选择".to_string(),
                options: vec![CardSelectOption {
                    text: "仅结果".to_string(),
                    value: "act:/push result_only".to_string(),
                }],
                init_value: String::new(),
                action: String::new(),
            })
            .build();

        let v = render_card_map(&card, "sk1");
        let select_static = &v["body"]["elements"]
            .as_array()
            .expect("elements 数组")
            .iter()
            .find(|e| e["tag"] == "action")
            .expect("找到 action 元素")["actions"][0];
        assert_eq!(select_static["value"]["session_key"], "sk1");
        // action 为空 → value 不含 action 键
        assert!(
            select_static["value"].get("action").is_none(),
            "action 为空时 value 不应有 action 字段"
        );
        // init_value 为空 → 不渲染 initial_option
        assert!(
            select_static.get("initial_option").is_none(),
            "init_value 为空时不应渲染 initial_option"
        );
    }

    /// 状态页（默认）：工作空间/状态/推送 + 新会话/停止。
    /// 注意：状态页精简后只展示状态条与控制按钮，最近任务与历史入口已移除
    /// （想看记录需要切到「事项 / 环路」或调用历史子页）。
    #[test]
    fn test_build_help_console_card_status_page() {
        let state = HelpCardState {
            current_group: "status".to_string(),
            workspace: Some(WorkspaceSummary {
                id: 1,
                name: "my-app".to_string(),
                executor: "pi".to_string(),
            }),
            is_running: true,
            push_level: "result_only".to_string(),
            recent_records: vec![RecentTaskItem {
                status_icon: "✅".to_string(),
                title: "修复登录bug".to_string(),
                time_desc: "2分钟前".to_string(),
            }],
            ..Default::default()
        };
        let json = render_card_map(&build_help_console_card(&state), "sk").to_string();
        // 状态条核心三要素
        assert!(json.contains("my-app"), "应显示当前工作空间名");
        assert!(json.contains("运行中"), "应显示运行状态");
        assert!(json.contains("仅结果"), "应显示推送级别");
        // 控制按钮：新会话 / 停止
        assert!(json.contains("act:/new") && json.contains("act:/stop"), "应有新会话/停止按钮");
        // 状态页精简后不再展示最近任务 / 历史入口，避免与生产实现不一致
        assert!(!json.contains("修复登录bug"), "状态页不应再渲染最近任务条目");
        assert!(!json.contains("nav:/history"), "状态页不应再有历史入口");
    }

    /// 状态页空状态：无工作空间 + 空闲 + 关闭推送。
    #[test]
    fn test_build_help_console_card_status_page_empty() {
        // 默认状态下 push_level 为空字符串 → push_level_label 走 "_" 分支返回「关闭」，
        // 此时状态页只展示「工作空间：未设置 / 运行状态：空闲 / 推送级别：关闭」三条文本。
        let state = HelpCardState { current_group: "status".to_string(), ..Default::default() };
        let json = render_card_map(&build_help_console_card(&state), "sk").to_string();
        assert!(json.contains("未设置"), "无工作空间时应显示未设置");
        assert!(json.contains("空闲"), "应显示空闲");
        assert!(json.contains("关闭"), "默认/关闭推送级别时应显示关闭");
    }

    /// 工作空间页：当前工作空间 + 列表[切换] + 推送 3 按钮 + 设为推送目标。
    #[test]
    fn test_build_help_console_card_workspace_page() {
        let state = HelpCardState {
            current_group: "workspace".to_string(),
            workspace: Some(WorkspaceSummary { id: 1, name: "my-app".to_string(), executor: "pi".to_string() }),
            workspaces: vec![
                WorkspaceItem { id: 1, name: "my-app".to_string(), is_current: true },
                WorkspaceItem { id: 2, name: "backend".to_string(), is_current: false },
            ],
            push_level: "result_only".to_string(),
            ..Default::default()
        };
        let json = render_card_map(&build_help_console_card(&state), "sk").to_string();
        assert!(json.contains("act:/bind 2"), "非当前工作空间应有切换按钮（按 id）");
        assert!(json.contains("act:/push result_only"), "应含推送级别按钮");
        assert!(json.contains("act:/sethome"), "应含设为推送目标");
        assert!(json.contains("pi"), "应显示执行器");
    }

    /// 事项页：当前 workspace 的事项列表 + [执行] 按钮。
    #[test]
    fn test_build_help_console_card_todo_page() {
        let state = HelpCardState {
            current_group: "todo".to_string(),
            todos: vec![TodoItem { id: 10, title: "整理文档".to_string(), status_icon: "⏸️".to_string() }],
            ..Default::default()
        };
        let json = render_card_map(&build_help_console_card(&state), "sk").to_string();
        assert!(json.contains("整理文档"), "应显示事项标题");
        assert!(json.contains("act:/runtodo 10"), "应有执行按钮（按 id）");
    }

    /// 环路页：当前 workspace 的环路列表 + [执行] 按钮。
    #[test]
    fn test_build_help_console_card_loop_page() {
        let state = HelpCardState {
            current_group: "loop".to_string(),
            loops: vec![LoopItem { id: 20, name: "每日巡检".to_string(), status: "enabled".to_string() }],
            ..Default::default()
        };
        let json = render_card_map(&build_help_console_card(&state), "sk").to_string();
        assert!(json.contains("每日巡检"), "应显示环路名");
        assert!(json.contains("act:/runloop 20"), "应有执行按钮（按 id）");
    }

    /// 历史子页：分页按钮 + 页码 + 上一页/下一页跳转。
    #[test]
    fn test_build_history_card_pagination() {
        let items = vec![HistoryItem {
            status_icon: "✅".to_string(),
            title: "任务A".to_string(),
            trigger: "manual".to_string(),
            time_desc: "刚刚".to_string(),
        }];
        let json = render_card_map(&build_history_card(&items, 2, 3), "sk").to_string();
        assert!(json.contains("任务A"));
        assert!(json.contains("上一页"), "非首页应有上一页");
        assert!(json.contains("下一页"), "非末页应有下一页");
        assert!(json.contains("nav:/history 1"), "上一页应跳到 page 1");
        assert!(json.contains("nav:/help task"), "应能返回控制台");
    }

    #[test]
    fn test_render_card_basic() {
        let card = CardBuilder::new()
            .title("Test", "blue")
            .markdown("Content")
            .build();

        let json = render_card(&card, "session123");
        assert!(json.contains("\"tag\":\"markdown\""));
        assert!(json.contains("\"content\":\"Content\""));
    }

    #[test]
    fn test_render_buttons_equal() {
        let card = CardBuilder::new()
            .title("Tabs", "blue")
            .buttons_equal(vec![
                CardButton::primary("Tab1", "nav:/tab1"),
                CardButton::default_btn("Tab2", "nav:/tab2"),
            ])
            .build();

        let json = render_card(&card, "");
        assert!(json.contains("\"tag\":\"column_set\""));
        assert!(json.contains("\"flex_mode\":\"bisect\""));
    }

    #[test]
    fn test_render_list_item() {
        let card = CardBuilder::new()
            .list_item("**/new**  Start a new session", "▶", "act:/new")
            .build();

        let json = render_card(&card, "");
        assert!(json.contains("\"tag\":\"column_set\""));
        assert!(json.contains("Start a new session"));
        assert!(json.contains("\"action\":\"act:/new\""));
    }

    // ========== 统一卡片消息测试 ==========

    #[test]
    fn test_build_success_card() {
        let card = build_success_card("操作成功", "任务已完成");
        assert!(card.header.is_some());
        let json = render_card(&card, "");
        assert!(json.contains("✅"));
        assert!(json.contains("操作成功"));
        assert!(json.contains("\"template\":\"green\""));
    }

    #[test]
    fn test_build_error_card() {
        let card = build_error_card("操作失败", "发生了错误");
        assert!(card.header.is_some());
        let json = render_card(&card, "");
        assert!(json.contains("❌"));
        assert!(json.contains("\"template\":\"red\""));
    }

    #[test]
    fn test_build_warning_card() {
        let card = build_warning_card("警告", "请注意");
        assert!(card.header.is_some());
        let json = render_card(&card, "");
        assert!(json.contains("⚠️"));
        assert!(json.contains("\"template\":\"orange\""));
    }

    #[test]
    fn test_build_info_card() {
        let card = build_info_card("提示", "信息如下");
        assert!(card.header.is_some());
        let json = render_card(&card, "");
        assert!(json.contains("ℹ️"));
        assert!(json.contains("\"template\":\"blue\""));
    }

    #[test]
    fn test_card_message_builder_with_action() {
        let card = CardMessageBuilder::new(CardMessageStatus::Info, "标题", "内容")
            .add_action("取消", "cmd:/cancel")
            .add_primary_action("确认", "cmd:/confirm")
            .set_footer("这是一条提示")
            .build();

        assert!(card.header.is_some());
        let json = render_card(&card, "");
        // 验证内容包含按钮
        assert!(json.contains("取消"));
        assert!(json.contains("确认"));
        assert!(json.contains("这是一条提示"));
    }

    #[test]
    fn test_card_message_status_icon() {
        let config = CardMessageConfig {
            status: CardMessageStatus::Success,
            title: "Test".to_string(),
            content: "Content".to_string(),
            actions: vec![],
            footer: None,
        };
        assert_eq!(config.status_icon(), "✅");
        assert_eq!(config.status_color(), "green");
    }
}
