// 飞书消息的会话维度元信息：把后端原始字段(chat_type / is_history)统一映射成
// 「中文标签 + 颜色」，供「消息监控台」的列表卡片与详情抽屉复用。
// 抽到一处是为了避免两处各自硬编码同一套文案与配色，且让群聊/私聊、历史/实时
// 这两个维度的视觉语义在列表与详情之间保持一致。

export interface MessageMeta {
  label: string;
  color: string;
}

// chat_type 来自飞书事件原始取值：group=群聊、p2p=单聊(私聊)；
// 历史消息回拉入库时该字段可能为空字符串，统一兜底为「未知」而非暴露内部取值。
const chatTypeLabelMap: Record<string, string> = {
  group: '群聊',
  p2p: '私聊',
};

// 把后端的 chat_type 映射成用户可读的标签与配色。
// 群聊用紫色、私聊用品红——二者是同一维度的互斥取值，用对比色让列表上一眼可分。
export function chatTypeMeta(chatType: string): MessageMeta {
  const label = chatTypeLabelMap[chatType] ?? '未知';
  const color = chatType === 'p2p' ? 'magenta' : chatType === 'group' ? 'purple' : 'default';
  return { label, color };
}

// is_history 区分消息来源时机：true=历史回拉(用户此前发送的旧消息)，
// false=实时接收到的当前消息。它是判断「实时群聊 / 历史群聊」等组合的另一个维度。
// 历史用金色(陈旧感)、实时用青色(新鲜感)，与「未处理=橙、已处理=绿」的处理状态配色错开。
export function historyMeta(isHistory: boolean): MessageMeta {
  return isHistory
    ? { label: '历史', color: 'gold' }
    : { label: '实时', color: 'cyan' };
}

export interface CopyableId {
  label: string;
  value: string;
}

// 返回该消息「可复制到推送目标配置」的 ID：
// - 群聊：取 chat_id，贴到推送目标的「群聊接收 ID」(receive_id_type=chat_id)；
// - 私聊：取发送者 open_id，贴到「单聊接收 ID」(receive_id_type=open_id)。
//   飞书的 p2p 消息虽也带 chat_id，但那是飞书给 1:1 会话分配的内部 ID，
//   私聊推送真正用的是 open_id，故私聊这里给 open_id 而非 chat_id；
//   若 open_id 缺失(异常数据)则回退到 chat_id，保证至少有内容可复制。
// - 类型未知时回退展示 chat_id，标签为「会话ID」。
// 返回 null 表示该消息没有可复制的 ID，调用方据此隐藏整行。
export function copyableIdForPush(
  chatType: string,
  chatId: string,
  senderOpenId: string,
): CopyableId | null {
  if (chatType === 'group') {
    return chatId ? { label: '群聊ID', value: chatId } : null;
  }
  if (chatType === 'p2p') {
    return { label: '单聊ID', value: senderOpenId || chatId };
  }
  return chatId ? { label: '会话ID', value: chatId } : null;
}
