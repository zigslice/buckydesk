const KEY = 'buckydesk.workspace.v1';

export function initialState() {
  return {
    phase: 'welcome',
    place: 'today',
    account: null,
    ready: false,
    formError: '',
    notice: '',
    files: [{ id: 'root', parentId: null, kind: 'folder', name: '我的文件', text: '' }],
    folderId: 'root',
    openFileId: null,
    query: '',
    creating: null,
    people: [],
    members: [],
    messages: [],
    agents: [],
    pickedFolders: [],
    agentDraft: { name: '', telegramId: '' },
    works: [],
    openWorkId: null,
    model: null,
    reach: null,
    otherComputer: '',
    language: '中文',
    resetArmed: false,
  };
}

export function loadState() {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return initialState();
    const data = JSON.parse(raw);
    return normalize(data);
  } catch {
    return initialState();
  }
}

export function saveState(state) {
  localStorage.setItem(KEY, JSON.stringify(state));
}

function normalize(data) {
  const base = initialState();
  if (!data || typeof data !== 'object') return base;
  const next = { ...base, ...data };
  if (!Array.isArray(next.files) || !next.files.some((file) => file.id === 'root')) {
    next.files = base.files;
  }
  if (next.phase === 'preparing') next.phase = 'partial';
  if (['desk', 'login', 'partial'].includes(next.phase) && !next.account?.name) {
    return base;
  }
  return next;
}

function uid(prefix) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

export function beginSetup(state, input) {
  const name = input.name.trim();
  const keyFolder = input.keyFolder.trim();
  if (!name) return { ...state, formError: '先写下你的名字。' };
  if (input.password !== input.confirm) return { ...state, formError: '两次输入的密码不一致' };
  if (input.password.length < 8) return { ...state, formError: '密码长度不能小于 8 位' };
  if (!keyFolder) return { ...state, formError: '先选定钥匙要放的位置。' };
  return {
    ...state,
    formError: '',
    notice: '',
    phase: 'preparing',
    ready: false,
    account: { name, password: input.password, keyFolder },
    members: [{ id: 'owner', name }],
  };
}

export function finishSetup(state) {
  if (state.phase !== 'preparing' && state.phase !== 'partial') return state;
  return { ...state, phase: 'desk', place: 'today', ready: true, formError: '', notice: '' };
}

export function resumeSetup(state) {
  if (state.phase !== 'partial') return state;
  return { ...state, phase: 'preparing', formError: '' };
}

export function login(state, password) {
  if (!state.account || password !== state.account.password) {
    return { ...state, formError: '密码不正确。' };
  }
  return { ...state, phase: 'desk', place: 'today', ready: true, formError: '', notice: '' };
}

export function logout(state) {
  return { ...state, phase: 'login', place: 'today', formError: '', notice: '', resetArmed: false };
}

export function go(state, place) {
  return {
    ...state,
    place,
    formError: '',
    notice: '',
    creating: null,
    openFileId: place === 'files' ? state.openFileId : null,
    resetArmed: false,
  };
}

export function childrenOf(state, parentId) {
  return state.files.filter((file) => file.parentId === parentId);
}

export function crumbs(state) {
  const chain = [];
  let id = state.folderId;
  const guard = new Set();
  while (id && !guard.has(id)) {
    guard.add(id);
    const node = state.files.find((file) => file.id === id);
    if (!node) break;
    chain.unshift(node);
    id = node.parentId;
  }
  return chain;
}

function sameName(state, parentId, name, exceptId) {
  return childrenOf(state, parentId).some((file) => file.id !== exceptId && file.name === name);
}

export function openFolder(state, id) {
  return { ...state, folderId: id, openFileId: null, formError: '', creating: null };
}

export function startCreate(state, creating) {
  return { ...state, creating, formError: '' };
}

export function addFolder(state, name) {
  const trimmed = name.trim();
  if (!trimmed) return { ...state, formError: '先写下名字。' };
  if (sameName(state, state.folderId, trimmed)) {
    return { ...state, formError: '这里已经有同名的东西。' };
  }
  return {
    ...state,
    formError: '',
    creating: null,
    files: [
      ...state.files,
      { id: uid('folder'), parentId: state.folderId, kind: 'folder', name: trimmed, text: '' },
    ],
  };
}

export function addFile(state, name, text) {
  const trimmed = name.trim();
  if (!trimmed) return { ...state, formError: '先写下名字。' };
  if (sameName(state, state.folderId, trimmed)) {
    return { ...state, formError: '这里已经有同名的东西。' };
  }
  const id = uid('file');
  return {
    ...state,
    formError: '',
    creating: null,
    openFileId: id,
    files: [
      ...state.files,
      { id, parentId: state.folderId, kind: 'file', name: trimmed, text: text ?? '' },
    ],
  };
}

export function openFile(state, id) {
  return { ...state, openFileId: id, formError: '' };
}

export function closeFile(state) {
  return { ...state, openFileId: null };
}

export function renameFile(state, id, name) {
  const trimmed = name.trim();
  const current = state.files.find((file) => file.id === id);
  if (!current || !trimmed) return { ...state, formError: '先写下名字。' };
  if (sameName(state, current.parentId, trimmed, id)) {
    return { ...state, formError: '这里已经有同名的东西。' };
  }
  return {
    ...state,
    formError: '',
    creating: null,
    files: state.files.map((file) => (file.id === id ? { ...file, name: trimmed } : file)),
  };
}

function descendantIds(files, id) {
  const drop = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const file of files) {
      if (file.parentId && drop.has(file.parentId) && !drop.has(file.id)) {
        drop.add(file.id);
        grew = true;
      }
    }
  }
  return drop;
}

export function deleteFile(state, id) {
  if (id === 'root') return state;
  const drop = descendantIds(state.files, id);
  return {
    ...state,
    formError: '',
    openFileId: drop.has(state.openFileId) ? null : state.openFileId,
    folderId: drop.has(state.folderId) ? 'root' : state.folderId,
    files: state.files.filter((file) => !drop.has(file.id)),
  };
}

export function moveFile(state, id, parentId) {
  if (id === 'root' || id === parentId) return state;
  const drop = descendantIds(state.files, id);
  if (drop.has(parentId)) return { ...state, formError: '不能移进自己里面。' };
  const current = state.files.find((file) => file.id === id);
  if (!current) return state;
  if (sameName(state, parentId, current.name, id)) {
    return { ...state, formError: '那里已经有同名的东西。' };
  }
  return {
    ...state,
    formError: '',
    files: state.files.map((file) => (file.id === id ? { ...file, parentId } : file)),
  };
}

export function searchFiles(state, query) {
  return { ...state, query: query.trim(), openFileId: null, formError: '' };
}

export function folders(state) {
  return state.files.filter((file) => file.kind === 'folder');
}

export function askSummary(state, fileId) {
  const file = state.files.find((item) => item.id === fileId && item.kind === 'file');
  if (!file) return state;
  if (!state.agents.length) {
    return { ...state, place: 'agent', notice: '先做一个 Agent。', formError: '' };
  }
  const agent = state.agents.find((item) => item.running);
  if (!agent) {
    return { ...state, place: 'agent', notice: 'Agent 停着。先让它开始。', formError: '' };
  }
  const allowed = agent.folderIds.includes(file.parentId) || agent.folderIds.includes('root');
  if (!allowed) {
    return { ...state, place: 'agent', notice: '这个 Agent 还不能看放文件的那个文件夹。', formError: '' };
  }
  if (!state.model) {
    return { ...state, place: 'model', notice: '先接上一个模型。接上之前也可以回到今天。', formError: '' };
  }
  const clip = file.text.trim().slice(0, 240);
  const body = clip
    ? `《${file.name}》\n\n${clip}\n\n这一版由页面整理开头。用量已经记下，模型服务还没有接上。`
    : `《${file.name}》已经放进工作。文件里还没有正文。`;
  const id = uid('work');
  return {
    ...state,
    place: 'work',
    openWorkId: id,
    notice: '',
    formError: '',
    model: { ...state.model, calls: (state.model.calls || 0) + 1 },
    works: [
      {
        id,
        title: `${file.name} 的摘要`,
        body,
        saved: body,
        history: [],
        status: 'done',
        seen: false,
      },
      ...state.works,
    ],
    messages: [
      {
        id: uid('msg'),
        from: '工作区',
        text: `《${file.name}》的摘要放在工作里了。`,
        reply: '',
        workId: id,
      },
      ...state.messages,
    ],
  };
}

export function dismissWork(state, id) {
  return {
    ...state,
    works: state.works.map((work) => (work.id === id ? { ...work, seen: true } : work)),
  };
}

export function openWork(state, id) {
  return { ...state, place: 'work', openWorkId: id, formError: '' };
}

export function editWork(state, id, body) {
  return {
    ...state,
    works: state.works.map((work) => (work.id === id ? { ...work, body } : work)),
  };
}

export function keepWork(state, id) {
  return {
    ...state,
    works: state.works.map((work) => {
      if (work.id !== id || work.body === work.saved) return work;
      return { ...work, history: [...work.history, work.saved].slice(-30), saved: work.body };
    }),
  };
}

export function undoWork(state, id) {
  return {
    ...state,
    works: state.works.map((work) => {
      if (work.id !== id) return work;
      if (work.body !== work.saved) return { ...work, body: work.saved };
      const prev = work.history[work.history.length - 1];
      if (prev == null) return work;
      return { ...work, history: work.history.slice(0, -1), body: prev, saved: prev };
    }),
  };
}

export function newWork(state, title) {
  const trimmed = title.trim();
  if (!trimmed) return { ...state, formError: '先写下标题。' };
  const id = uid('work');
  const body = '';
  return {
    ...state,
    formError: '',
    creating: null,
    openWorkId: id,
    works: [{ id, title: trimmed, body, saved: body, history: [], status: 'done', seen: true }, ...state.works],
  };
}

export function toggleFolder(state, id) {
  const has = state.pickedFolders.includes(id);
  return {
    ...state,
    pickedFolders: has ? state.pickedFolders.filter((item) => item !== id) : [...state.pickedFolders, id],
  };
}

export function createAgent(state, input) {
  const name = input.name.trim();
  if (!name) return { ...state, formError: '先写下 Agent 的名字。' };
  const telegramId = input.telegramId.trim();
  if (telegramId && !/^\d+$/.test(telegramId)) {
    return { ...state, formError: 'Telegram 用户 ID 只能包含数字。' };
  }
  if (!state.pickedFolders.length) {
    return { ...state, formError: '勾上它能看的文件夹。' };
  }
  return {
    ...state,
    formError: '',
    creating: null,
    notice: '',
    pickedFolders: [],
    agentDraft: { name: '', telegramId: '' },
    agents: [
      ...state.agents,
      {
        id: uid('agent'),
        name,
        folderIds: state.pickedFolders,
        running: true,
        telegramId,
      },
    ],
  };
}

export function toggleAgent(state, id) {
  return {
    ...state,
    agents: state.agents.map((agent) =>
      agent.id === id ? { ...agent, running: !agent.running } : agent
    ),
  };
}

export function saveTelegram(state, id, telegramId) {
  const trimmed = telegramId.trim();
  if (trimmed && !/^\d+$/.test(trimmed)) {
    return { ...state, formError: 'Telegram 用户 ID 只能包含数字。' };
  }
  return {
    ...state,
    formError: '',
    agents: state.agents.map((agent) => (agent.id === id ? { ...agent, telegramId: trimmed } : agent)),
  };
}

export function addPerson(state, name) {
  const trimmed = name.trim();
  if (!trimmed) return { ...state, formError: '先写下名字。' };
  return {
    ...state,
    formError: '',
    creating: null,
    people: [...state.people, { id: uid('person'), name: trimmed, muted: false }],
  };
}

export function toggleMute(state, id) {
  return {
    ...state,
    people: state.people.map((person) =>
      person.id === id ? { ...person, muted: !person.muted } : person
    ),
  };
}

export function replyMessage(state, id, reply) {
  const trimmed = reply.trim();
  if (!trimmed) return { ...state, formError: '先写下答复。' };
  return {
    ...state,
    formError: '',
    messages: state.messages.map((message) =>
      message.id === id ? { ...message, reply: trimmed } : message
    ),
  };
}

export function connectModel(state, provider, key) {
  const name = provider.trim();
  const secret = key.trim();
  if (!name || !secret) return { ...state, formError: '写下服务的名字和密钥。' };
  return {
    ...state,
    formError: '',
    notice: '',
    creating: null,
    model: { provider: name, key: secret, calls: state.model?.calls || 0 },
  };
}

export function disconnectModel(state) {
  return { ...state, model: null, formError: '' };
}

export function chooseReach(state, kind, value) {
  if (kind === 'local') {
    return { ...state, reach: { kind, value: state.account?.name || '' }, formError: '' };
  }
  const domain = value.trim();
  if (!domain || !domain.includes('.')) return { ...state, formError: '域名格式不正确' };
  return { ...state, reach: { kind: 'domain', value: domain }, formError: '' };
}

export function noteOtherComputer(state, address) {
  const trimmed = address.trim();
  if (!trimmed) return { ...state, formError: '先写下那台电脑的地址。' };
  return { ...state, otherComputer: trimmed, formError: '', notice: '已记下。那台电脑准备好之后，在这里继续。' };
}

export function changePassword(state, input) {
  if (!state.account || input.current !== state.account.password) {
    return { ...state, formError: '现在的密码不正确。' };
  }
  if (input.next !== input.confirm) return { ...state, formError: '两次输入的密码不一致' };
  if (input.next.length < 8) return { ...state, formError: '密码长度不能小于 8 位' };
  return {
    ...state,
    formError: '',
    notice: '密码已更新。',
    account: { ...state.account, password: input.next },
  };
}

export function setLanguage(state, language) {
  return {
    ...state,
    language,
    notice: language === '中文' ? '' : '语言已记下。这一版页面仍是中文。',
  };
}

export function addMember(state, name) {
  const trimmed = name.trim();
  if (!trimmed) return { ...state, formError: '先写下名字。' };
  return {
    ...state,
    formError: '',
    creating: null,
    members: [...state.members, { id: uid('member'), name: trimmed }],
  };
}

export function armReset(state, armed) {
  return { ...state, resetArmed: armed, formError: '' };
}

export function resetAll() {
  localStorage.removeItem(KEY);
  return initialState();
}

export function todayItems(state) {
  const items = [];
  for (const work of state.works) {
    if (work.seen) continue;
    if (work.status === 'done' || work.status === 'failed') {
      items.push({
        id: work.id,
        kind: 'work',
        title: work.title,
        detail: work.status === 'failed' ? '没有完成' : '可以打开',
      });
    }
  }
  for (const message of state.messages) {
    if (message.reply) continue;
    items.push({ id: message.id, kind: 'message', title: message.from, detail: message.text });
  }
  return items;
}

export function maskKey(key) {
  if (!key) return '';
  if (key.length <= 4) return '••••';
  return `••••${key.slice(-4)}`;
}
