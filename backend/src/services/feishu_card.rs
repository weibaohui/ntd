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
                    "tag": "note",
                    "elements": [{
                        "tag": "plain_text",
                        "content": note.text
                    }]
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
        CardActionLayout::EqualColumns => {
            // 等宽列布局：每个按钮一个 column，2个按钮时用 bisect
            let mut columns = Vec::new();
            for btn in &actions.buttons {
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
            if actions.buttons.len() == 2 {
                column_set["flex_mode"] = serde_json::json!("bisect");
            }

            vec![column_set]
        }
        CardActionLayout::Row => {
            // 普通行布局
            let mut btn_values = Vec::new();
            for btn in &actions.buttons {
                btn_values.push(render_button(btn, session_key, false));
            }
            vec![serde_json::json!({
                "tag": "action",
                "actions": btn_values
            })]
        }
    }
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
    let options: Vec<Value> = select.options.iter().map(|opt| {
        serde_json::json!({
            "text": {
                "tag": "plain_text",
                "content": opt.text
            },
            "value": opt.value
        })
    }).collect();

    let mut elem = serde_json::json!({
        "tag": "select_static",
        "placeholder": {
            "tag": "plain_text",
            "content": select.placeholder
        },
        "options": options
    });

    if !session_key.is_empty() {
        elem["value"] = serde_json::json!({
            "session_key": session_key
        });
    }

    if !select.init_value.is_empty() {
        elem["initial_option"] = serde_json::json!(select.init_value);
    }

    serde_json::json!({
        "tag": "action",
        "actions": [elem]
    })
}

// ============================================================================
// 辅助函数：构建 Help 卡片
// ============================================================================

/// Help 卡片分组定义
pub struct HelpGroup {
    pub key: &'static str,
    pub title: &'static str,
    pub items: Vec<HelpItem>,
}

/// Help 卡片项
pub struct HelpItem {
    pub command: &'static str,
    pub action: &'static str,
    pub description: &'static str,
}

/// 构建 Help 卡片
pub fn build_help_card(current_group: &str, groups: &[HelpGroup]) -> Card {
    // 找到当前分组
    let current = groups.iter()
        .find(|g| g.key == current_group)
        .unwrap_or(&groups[0]);

    // 构建 Tab 按钮
    let mut tabs = Vec::new();
    for group in groups {
        let btn_type = if group.key == current.key { "primary" } else { "default" };
        tabs.push(CardButton::new(group.title, btn_type, &format!("nav:/help {}", group.key)));
    }

    // 将 Tab 按钮每2个一行
    let tab_rows: Vec<Vec<CardButton>> = tabs.chunks(2).map(|chunk| chunk.to_vec()).collect();

    // 使用 Builder 构建卡片
    let mut builder = CardBuilder::new()
        .title("NTD 帮助", "blue");

    // 添加 Tab 按钮行
    for row in tab_rows {
        builder = builder.buttons_equal(row);
    }

    // 添加分隔线
    builder = builder.divider();

    // 添加当前分组的项
    for item in &current.items {
        let text = format!("**{}**  {}", item.command, item.description);
        builder = builder.list_item(&text, "▶", item.action);
    }

    // 添加提示
    builder = builder.note("💡 点击按钮可快速执行操作 | 发送 /help 查看所有命令");

    builder.build()
}

/// NTD 飞书 Bot Help 分组定义
/// 参考 cc-connect 的 helpCardGroups 设计，按功能分为多个 Tab 分组
pub fn help_groups() -> Vec<HelpGroup> {
    vec![
        HelpGroup {
            key: "common",
            title: "常用",
            items: vec![
                HelpItem { command: "/help", action: "nav:/help common", description: "显示帮助信息" },
                HelpItem { command: "/list", action: "cmd:/list", description: "查看已绑定项目" },
                HelpItem { command: "/new", action: "cmd:/new", description: "开启新会话" },
                HelpItem { command: "/stop", action: "cmd:/stop", description: "停止当前任务" },
            ],
        },
        HelpGroup {
            key: "binding",
            title: "绑定",
            items: vec![
                HelpItem { command: "/bind", action: "cmd:/bind", description: "绑定项目到当前聊天" },
                HelpItem { command: "/unbind", action: "cmd:/unbind", description: "解绑当前项目" },
                HelpItem { command: "/sethome", action: "cmd:/sethome", description: "设置推送目标" },
            ],
        },
        HelpGroup {
            key: "push",
            title: "推送",
            items: vec![
                HelpItem { command: "/feishupush", action: "cmd:/feishupush", description: "切换推送模式" },
            ],
        },
    ]
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

    #[test]
    fn test_help_card_groups() {
        let groups = help_groups();
        let card = build_help_card("common", &groups);
        assert!(card.header.is_some());
        // Tab buttons (EqualColumns) + Divider + ListItem = 3 element groups
        assert!(!card.elements.is_empty());
    }

    #[test]
    fn test_help_card_unknown_group_defaults_to_first() {
        let groups = help_groups();
        let card = build_help_card("nonexistent", &groups);
        assert!(card.header.is_some());
        // 应该默认显示第一个分组 (common)
        assert!(!card.elements.is_empty());
    }

    #[test]
    fn test_help_card_json_rendering() {
        let groups = help_groups();
        let card = build_help_card("common", &groups);
        let json = render_card(&card, "test_session");
        // 验证基本结构（JSON 2.0 格式）
        assert!(json.contains("\"schema\":\"2.0\""));
        assert!(json.contains("\"body\""));
        assert!(json.contains("\"header\""));
        assert!(json.contains("\"elements\""));
        assert!(json.contains("NTD 帮助"));
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
