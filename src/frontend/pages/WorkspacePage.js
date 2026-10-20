import { Component } from '/gd5/src/engine/index.js';
import { Button, Card, Checkbox, Field, Input, Text, TextArea, Title, loadUi } from '/gd5/src/ui/index.js';
import { el } from '/gd5/src/ui/util.js';
import { exec } from '../workspace/api.js';
import {
  addFile,
  addFolder,
  addMember,
  addPerson,
  applyBoot,
  armReset,
  askSummary,
  beginSetup,
  chooseReach,
  closeFile,
  connectModel,
  createAgent,
  crumbs,
  childrenOf,
  deleteFile,
  disconnectModel,
  dismissWork,
  editWork,
  enterDesk,
  finishSetup,
  folders,
  go,
  keepWork,
  loadState,
  logout,
  maskKey,
  moveFile,
  newWork,
  nextPasswordError,
  noteOtherComputer,
  openFile,
  openFolder,
  openWork,
  renameFile,
  replyMessage,
  resetAll,
  resumeSetup,
  saveState,
  saveTelegram,
  searchFiles,
  setLanguage,
  startCreate,
  todayItems,
  toggleAgent,
  toggleFolder,
  toggleMute,
  undoWork,
} from '../workspace/store.js';

const PLACES = [
  ['today', '今天'],
  ['files', '文件'],
  ['messages', '消息'],
  ['people', '我认识的人'],
  ['work', '工作'],
  ['agent', 'Agent'],
  ['model', '模型'],
  ['computer', '这台电脑'],
];

export class WorkspacePage extends Component {
  static styles = new URL('./WorkspacePage.css', import.meta.url);

  async componentDidMount() {
    await loadUi();
    const local = loadState();
    try {
      const identity = await exec(['identity', 'status']);
      const kernel = await exec(['kernel', 'status']);
      const next = applyBoot(local, identity, kernel);
      saveState(next);
      this.state.data = next;
    } catch {
      this.state.data = { ...local, phase: 'unavailable', formError: '', notice: '' };
    }
  }

  componentDidUnMount() {
    this._prep = (this._prep || 0) + 1;
    clearTimeout(this._timer);
  }

  commit(next) {
    saveState(next);
    this.state.data = next;
    this.refresh();
  }

  field(id) {
    return document.getElementById(id)?.value ?? '';
  }

  prepare() {
    clearTimeout(this._timer);
    this._prep = (this._prep || 0) + 1;
    const token = this._prep;
    this._timer = setTimeout(() => {
      if (token !== this._prep) return;
      void this.finishPreparing();
    }, 700);
  }

  async start() {
    if (this._setup) return;
    this._setup = true;
    try {
      const input = {
        name: this.field('name'),
        password: this.field('password'),
        confirm: this.field('confirm'),
        keyFolder: this.field('keyFolder'),
      };
      const next = beginSetup(this.state.data, input);
      if (next.phase !== 'preparing') {
        next.welcomeDraft = input;
        this.commit(next);
        return;
      }
      this.commit(next);
      let report;
      try {
        report = await exec([
          'identity',
          'activate',
          '--name',
          input.name.trim(),
          '--password',
          input.password,
          '--key-folder',
          input.keyFolder.trim(),
        ]);
      } catch {
        this.commit({ ...this.state.data, phase: 'unavailable' });
        return;
      }
      if (!report.ok) {
        const phase = report.summary === '上次没有完成。' ? 'partial' : 'welcome';
        this.commit({
          ...this.state.data,
          phase,
          formError: report.summary,
          welcomeDraft: phase === 'welcome' ? input : undefined,
        });
        return;
      }
      this.prepare();
    } finally {
      this._setup = false;
    }
  }

  continueSetup() {
    this.commit(resumeSetup(this.state.data));
    this.prepare();
  }

  async finishPreparing() {
    if (!this.state.data || this.state.data.phase !== 'preparing') return;
    let report;
    try {
      report = await exec(['identity', 'finish']);
    } catch {
      this.commit({ ...this.state.data, phase: 'unavailable' });
      return;
    }
    if (!this.state.data || this.state.data.phase !== 'preparing') return;
    if (!report.ok) {
      this.commit({
        ...this.state.data,
        phase: report.summary === '还没有开始准备。' ? 'welcome' : 'partial',
        formError: report.summary,
      });
      return;
    }
    const next = finishSetup(this.state.data);
    next.account = {
      name: report.data.name || next.account?.name,
      keyFolder: report.data.keyFolder || next.account?.keyFolder,
    };
    next.ready = true;
    this.commit(next);
  }

  async signIn() {
    let report;
    try {
      report = await exec(['identity', 'login', '--password', this.field('password')]);
    } catch {
      this.commit({ ...this.state.data, formError: '工作区暂时打不开。' });
      return;
    }
    if (!report.ok) {
      this.commit({ ...this.state.data, formError: report.summary || '密码不正确。' });
      return;
    }
    this.commit(enterDesk(this.state.data));
  }

  async updatePassword() {
    const input = {
      current: this.field('currentPassword'),
      next: this.field('nextPassword'),
      confirm: this.field('nextConfirm'),
    };
    const error = nextPasswordError(input);
    if (error) {
      this.commit({ ...this.state.data, formError: error });
      return;
    }
    let report;
    try {
      report = await exec([
        'identity',
        'password',
        '--current',
        input.current,
        '--next',
        input.next,
      ]);
    } catch {
      this.commit({ ...this.state.data, formError: '工作区暂时打不开。' });
      return;
    }
    if (!report.ok) {
      this.commit({ ...this.state.data, formError: report.summary });
      return;
    }
    this.commit({ ...this.state.data, formError: '', notice: '密码已更新。' });
  }

  async resetDesk() {
    try {
      const report = await exec(['identity', 'reset']);
      if (!report.ok) {
        this.commit({ ...this.state.data, formError: report.summary });
        return;
      }
      const kernel = await exec(['kernel', 'status']);
      const next = resetAll();
      next.kernel = {
        attached: Boolean(kernel.data?.attached),
        summary: kernel.summary || '',
      };
      if (next.kernel.attached) next.phase = 'occupied';
      this.commit(next);
    } catch {
      this.commit({ ...this.state.data, formError: '工作区暂时打不开。' });
    }
  }

  render() {
    const state = this.state.data?.phase ? this.state.data : loadState();
    this.paint(({ div }) => {
      div({ id: 'root', className: 'desk' }, screen(this, state));
    });
  }
}

function screen(page, state) {
  if (state.phase === 'unavailable') return waiting('工作区暂时打不开。');
  if (state.phase === 'occupied') return occupied();
  if (state.phase === 'welcome') return welcome(page, state);
  if (state.phase === 'preparing') return waiting('正在准备你的工作区。');
  if (state.phase === 'partial') return partial(page);
  if (state.phase === 'login') return signIn(page, state);
  return desk(page, state);
}

function welcome(page, state) {
  return Card(
    { title: 'BuckyDesk' },
    Text({ text: '这台电脑会成为你的工作区。' }),
    Text({
      type: 'secondary',
      text: '名字和密码会记在这台电脑上。钥匙文件夹只记下路径。',
    }),
    formError(state),
    stack(
      Field({ label: '你的名字' }, Input({ id: 'name', value: state.welcomeDraft?.name || '' })),
      Field({ label: '管理密码' }, Input({ id: 'password', type: 'password', value: state.welcomeDraft?.password || '' })),
      Field({ label: '再输入一次' }, Input({ id: 'confirm', type: 'password', value: state.welcomeDraft?.confirm || '' })),
      Field(
        { label: '钥匙放在哪里', hint: '选一个你记得住的文件夹' },
        Input({
          id: 'keyFolder',
          placeholder: '例如 文档/buckydesk-key',
          value: state.welcomeDraft?.keyFolder || '',
        })
      ),
      Button({
        type: 'primary',
        text: '开始',
        onClick: () => page.start(),
      })
    )
  );
}

function waiting(text) {
  return Card({ title: 'BuckyDesk' }, Text({ text }));
}

function partial(page) {
  return Card(
    { title: 'BuckyDesk' },
    Text({ text: '上次没有完成。停在这里，不会另外做一套。' }),
    Button({
      type: 'primary',
      text: '继续准备',
      onClick: () => page.continueSetup(),
    })
  );
}

function occupied() {
  return Card({ title: 'BuckyDesk' }, Text({ text: '这台电脑上已经有一套 BuckyOS 在运行。' }));
}

function signIn(page, state) {
  return Card(
    { title: 'BuckyDesk' },
    Text({ text: state.account.name }),
    formError(state),
    stack(
      Field({ label: '管理密码' }, Input({ id: 'password', type: 'password' })),
      Button({
        type: 'primary',
        text: '进入',
        onClick: () => page.signIn(),
      })
    )
  );
}

function desk(page, state) {
  return [
    Title({ level: 1, text: 'BuckyDesk' }),
    Text({ type: 'secondary', text: `${state.account.name} 的工作区` }),
    divNav(page, state),
    state.notice && state.place !== 'computer' ? Text({ text: state.notice }) : null,
    place(page, state),
  ];
}

function divNav(page, state) {
  return el(
    'div',
    { className: 'nav' },
    PLACES.map(([id, label]) =>
      Button({
        type: state.place === id ? 'primary' : 'default',
        text: label,
        onClick: () => page.commit(go(page.state.data, id)),
      })
    )
  );
}

function place(page, state) {
  if (state.place === 'files') return files(page, state);
  if (state.place === 'messages') return messages(page, state);
  if (state.place === 'people') return people(page, state);
  if (state.place === 'work') return works(page, state);
  if (state.place === 'agent') return agents(page, state);
  if (state.place === 'model') return model(page, state);
  if (state.place === 'computer') return computer(page, state);
  return today(page, state);
}

function today(page, state) {
  const items = todayItems(state);
  return Card(
    { title: '今天' },
    items.length
      ? items.map((item) =>
          entry(
            item.title,
            item.detail,
            item.kind === 'work'
              ? [
                  Button({
                    type: 'primary',
                    text: '打开',
                    onClick: () => page.commit(openWork(page.state.data, item.id)),
                  }),
                  Button({
                    text: '知道了',
                    onClick: () => page.commit(dismissWork(page.state.data, item.id)),
                  }),
                ]
              : Button({
                  type: 'primary',
                  text: '去答复',
                  onClick: () => page.commit(go(page.state.data, 'messages')),
                })
          )
        )
      : Text({ text: '下一步可以把文件放进来，或者先做一个 Agent。' }),
    elRow([
      Button({ text: '去文件', onClick: () => page.commit(go(page.state.data, 'files')) }),
      Button({ text: '做 Agent', onClick: () => page.commit(go(page.state.data, 'agent')) }),
    ]),
    Text({ type: 'secondary', text: '要读的' }),
    Text({ type: 'muted', text: '还没有要读的。' }),
    state.kernel?.summary ? Text({ type: 'secondary', text: state.kernel.summary }) : null
  );
}

function files(page, state) {
  const query = state.query;
  const listed = query
    ? state.files.filter(
        (file) =>
          file.id !== 'root' &&
          (file.name.includes(query) || (file.text && file.text.includes(query)))
      )
    : childrenOf(state, state.folderId);
  const open = state.files.find((file) => file.id === state.openFileId);
  return Card(
    { title: '文件' },
    Text({ type: 'secondary', text: '这是你的文件。这一版只记在这个浏览器里。' }),
    formError(state),
    elRow(
      crumbs(state).map((crumb) =>
        Button({
          type: crumb.id === state.folderId ? 'primary' : 'text',
          text: crumb.name,
          onClick: () => page.commit(openFolder(page.state.data, crumb.id)),
        })
      )
    ),
    elRow([
      Input({ id: 'query', value: state.query, placeholder: '按名字或正文找' }),
      Button({
        text: '查找',
        onClick: () => page.commit(searchFiles(page.state.data, page.field('query'))),
      }),
      query
        ? Button({ text: '清除', onClick: () => page.commit(searchFiles(page.state.data, '')) })
        : null,
    ]),
    elRow([
      Button({ text: '新建文件夹', onClick: () => page.commit(startCreate(page.state.data, 'folder')) }),
      Button({ text: '新建文稿', onClick: () => page.commit(startCreate(page.state.data, 'doc')) }),
      Button({ text: '放进一个文件', onClick: () => page.ids().picker?.click() }),
    ]),
    Input({
      id: 'picker',
      type: 'file',
      style: 'display:none',
      onChange: (event) => {
        const file = event.target.files?.[0];
        if (file) page.ingest(file);
        event.target.value = '';
      },
    }),
    state.creating === 'folder' || state.creating === 'doc'
      ? elRow([
          Input({
            id: 'createName',
            placeholder: state.creating === 'folder' ? '文件夹名字' : '文稿名字',
          }),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () => {
              const name = page.field('createName');
              const next =
                page.state.data.creating === 'folder'
                  ? addFolder(page.state.data, name)
                  : addFile(page.state.data, name, '');
              page.commit(next);
            },
          }),
        ])
      : null,
    listed.length
      ? listed.map((file) => fileRow(page, state, file))
      : Text({ type: 'muted', text: query ? '没有找到。' : '这个文件夹是空的。' }),
    open && open.kind === 'file'
      ? Card(
          { title: open.name },
          Text({ className: 'preview', text: open.text || '还没有正文。' }),
          elRow([
            Button({ text: '收起', onClick: () => page.commit(closeFile(page.state.data)) }),
            Button({
              type: 'primary',
              text: '做成摘要，放到工作里',
              onClick: () => page.commit(askSummary(page.state.data, open.id)),
            }),
          ])
        )
      : null
  );
}

function fileRow(page, state, file) {
  const renaming = state.creating === `rename:${file.id}`;
  return entry(
    file.name,
    file.kind === 'folder' ? '文件夹' : '文稿',
    renaming
      ? [
          Input({ id: 'renameName', value: file.name }),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () => page.commit(renameFile(page.state.data, file.id, page.field('renameName'))),
          }),
        ]
      : [
          file.kind === 'folder'
            ? Button({
                text: '打开',
                onClick: () => page.commit(openFolder(page.state.data, file.id)),
              })
            : Button({ text: '看一眼', onClick: () => page.commit(openFile(page.state.data, file.id)) }),
          Button({
            text: '改名',
            onClick: () => page.commit(startCreate(page.state.data, `rename:${file.id}`)),
          }),
          moveSelect(page, state, file),
          file.id === 'root'
            ? null
            : Button({
                text: '删除',
                onClick: () => page.commit(deleteFile(page.state.data, file.id)),
              }),
        ]
  );
}

function moveSelect(page, state, file) {
  if (file.id === 'root') return null;
  const parent = state.files.find((item) => item.id === file.parentId);
  if (!parent?.parentId) return null;
  return Button({
    text: '移到上一层',
    onClick: () => page.commit(moveFile(page.state.data, file.id, parent.parentId)),
  });
}

function messages(page, state) {
  const pending = state.messages.filter((message) => !message.reply);
  const done = state.messages.filter((message) => message.reply);
  return Card(
    { title: '消息' },
    formError(state),
    pending.length ? null : Text({ text: '没有要答复的消息。' }),
    pending.map((message) =>
      Card(
        { title: message.from },
        Text({ text: message.text }),
        elRow([
          Input({ id: `reply-${message.id}`, placeholder: '答复' }),
          Button({
            type: 'primary',
            text: '答复',
            onClick: () =>
              page.commit(replyMessage(page.state.data, message.id, page.field(`reply-${message.id}`))),
          }),
        ])
      )
    ),
    done.map((message) => entry(message.from, `已答复：${message.reply}`, null))
  );
}

function people(page, state) {
  return Card(
    { title: '我认识的人' },
    Text({ type: 'secondary', text: '静音只影响今天的阅读，人还在，也不能因此进出这台电脑。' }),
    formError(state),
    Button({ text: '加一个人', onClick: () => page.commit(startCreate(page.state.data, 'person')) }),
    state.creating === 'person'
      ? elRow([
          Input({ id: 'personName', placeholder: '名字' }),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () => page.commit(addPerson(page.state.data, page.field('personName'))),
          }),
        ])
      : null,
    state.people.length ? null : Text({ type: 'muted', text: '还没有人。' }),
    state.people.map((person) =>
      entry(person.name, person.muted ? '今天不打扰' : '认识', [
        Button({
          text: person.muted ? '取消静音' : '静音',
          onClick: () => page.commit(toggleMute(page.state.data, person.id)),
        }),
      ])
    )
  );
}

function works(page, state) {
  const open = state.works.find((work) => work.id === state.openWorkId);
  return Card(
    { title: '工作' },
    Text({ type: 'secondary', text: '留下的修改可以撤销。' }),
    formError(state),
    Button({ text: '新的一份', onClick: () => page.commit(startCreate(page.state.data, 'work')) }),
    state.creating === 'work'
      ? elRow([
          Input({ id: 'workTitle', placeholder: '标题' }),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () => page.commit(newWork(page.state.data, page.field('workTitle'))),
          }),
        ])
      : null,
    state.works.map((work) =>
      entry(work.title, work.status === 'failed' ? '没有完成' : '可以打开', [
        Button({ text: '打开', onClick: () => page.commit(openWork(page.state.data, work.id)) }),
      ])
    ),
    open
      ? Card(
          { title: open.title },
          TextArea({
            id: 'workBody',
            className: 'editor',
            value: open.body,
            onInput: (event) => {
              const next = editWork(page.state.data, open.id, event.target.value);
              saveState(next);
              page.state.data = next;
            },
          }),
          elRow([
            Button({
              type: 'primary',
              text: '留下',
              onClick: () => page.commit(keepWork(page.state.data, open.id)),
            }),
            Button({
              text: '撤销',
              onClick: () => page.commit(undoWork(page.state.data, open.id)),
            }),
          ])
        )
      : null
  );
}

function agents(page, state) {
  return Card(
    { title: 'Agent' },
    state.notice ? null : Text({ type: 'secondary', text: '给它名字，并勾上它能看的文件夹。' }),
    formError(state),
    Button({ text: '做一个 Agent', onClick: () => page.commit(startCreate(page.state.data, 'agent')) }),
    state.creating === 'agent'
      ? stack(
          Field({ label: '名字' }, Input({ id: 'agentName', value: state.agentDraft?.name || '' })),
          Text({ type: 'secondary', text: '它能看的文件夹' }),
          ...folders(state).map((folder) =>
            Checkbox({
              label: folder.name,
              checked: state.pickedFolders.includes(folder.id),
              onChange: () => {
                const next = toggleFolder(page.state.data, folder.id);
                next.agentDraft = {
                  name: page.field('agentName'),
                  telegramId: page.field('telegramId'),
                };
                page.commit(next);
              },
            })
          ),
          Field(
            { label: '你的 Telegram 用户 ID', hint: '可选。Bot 以后再设，这里只认你本人。' },
            Input({ id: 'telegramId', value: state.agentDraft?.telegramId || '' })
          ),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () =>
              page.commit(
                createAgent(page.state.data, {
                  name: page.field('agentName'),
                  telegramId: page.field('telegramId'),
                })
              ),
          })
        )
      : null,
    state.agents.length ? null : Text({ type: 'muted', text: '还没有 Agent。' }),
    state.agents.map((agent) =>
      Card(
        { title: agent.name },
        Text({ text: agent.running ? '在工作' : '停着' }),
        Text({
          type: 'secondary',
          text: `能看 ${agent.folderIds.length} 个文件夹`,
        }),
        elRow([
          Button({
            text: agent.running ? '停下' : '开始',
            onClick: () => page.commit(toggleAgent(page.state.data, agent.id)),
          }),
        ]),
        elRow([
          Input({
            id: `tg-${agent.id}`,
            value: agent.telegramId || '',
            placeholder: 'Telegram 用户 ID',
          }),
          Button({
            text: '记下',
            onClick: () =>
              page.commit(saveTelegram(page.state.data, agent.id, page.field(`tg-${agent.id}`))),
          }),
        ])
      )
    )
  );
}

function model(page, state) {
  return Card(
    { title: '模型' },
    Text({ type: 'secondary', text: '没有模型时，Agent 先不做事。你也可以回到今天。' }),
    formError(state),
    state.model
      ? [
          Text({ text: state.model.provider }),
          Text({ type: 'secondary', text: `密钥 ${maskKey(state.model.key)}` }),
          Text({ text: `这个月大约用了 ${state.model.calls || 0} 次` }),
          Button({ text: '断开', onClick: () => page.commit(disconnectModel(page.state.data)) }),
        ]
      : stack(
          Field({ label: '服务' }, Input({ id: 'provider', placeholder: '例如一家你已经有密钥的服务' })),
          Field({ label: '密钥' }, Input({ id: 'modelKey', type: 'password' })),
          Button({
            type: 'primary',
            text: '接上',
            onClick: () =>
              page.commit(connectModel(page.state.data, page.field('provider'), page.field('modelKey'))),
          })
        )
  );
}

function computer(page, state) {
  return Card(
    { title: '这台电脑' },
    Text({ text: state.ready ? '工作区已就绪。' : '工作区还没准备好。' }),
    state.kernel?.summary ? Text({ text: state.kernel.summary }) : null,
    Text({ type: 'secondary', text: '名字和密码记在这台电脑上。文件、消息、Agent 和模型记在这个浏览器里。' }),
    Text({ type: 'secondary', text: '钥匙文件夹只记下路径。' }),
    formError(state),
    state.notice ? Text({ text: state.notice }) : null,
    Text({ text: `名字：${state.account.name}` }),
    Text({ text: `钥匙的位置：${state.account.keyFolder}` }),
    Text({ type: 'secondary', text: '这台电脑上的人' }),
    state.members.map((member) => entry(member.name, member.id === 'owner' ? '管理员' : '能进入', null)),
    state.creating === 'member'
      ? elRow([
          Input({ id: 'memberName', placeholder: '名字' }),
          Button({
            type: 'primary',
            text: '留下',
            onClick: () => page.commit(addMember(page.state.data, page.field('memberName'))),
          }),
        ])
      : Button({ text: '再加一个人', onClick: () => page.commit(startCreate(page.state.data, 'member')) }),
    Text({ type: 'secondary', text: '改密码' }),
    stack(
      Field({ label: '现在的密码' }, Input({ id: 'currentPassword', type: 'password' })),
      Field({ label: '新密码' }, Input({ id: 'nextPassword', type: 'password' })),
      Field({ label: '再输入一次' }, Input({ id: 'nextConfirm', type: 'password' })),
      Button({
        text: '更新密码',
        onClick: () => page.updatePassword(),
      })
    ),
    Text({ type: 'secondary', text: '语言' }),
    elRow([
      Button({
        type: state.language === '中文' ? 'primary' : 'default',
        text: '中文',
        onClick: () => page.commit(setLanguage(page.state.data, '中文')),
      }),
      Button({
        type: state.language === 'English' ? 'primary' : 'default',
        text: 'English',
        onClick: () => page.commit(setLanguage(page.state.data, 'English')),
      }),
    ]),
    Text({ type: 'secondary', text: '在别处打开' }),
    Text({
      type: 'muted',
      text: state.reach
        ? state.reach.kind === 'domain'
          ? `已记下域名 ${state.reach.value}。别人还不能从网上打开。`
          : `已记下名字 ${state.reach.value}。别人还不能从网上打开。`
        : '还只有这台电脑的浏览器能打开。',
    }),
    elRow([
      Button({
        text: '用我的名字',
        onClick: () => page.commit(chooseReach(page.state.data, 'local', '')),
      }),
      Input({ id: 'domain', placeholder: '自己的域名' }),
      Button({
        text: '用这个域名',
        onClick: () => page.commit(chooseReach(page.state.data, 'domain', page.field('domain'))),
      }),
    ]),
    Text({ type: 'secondary', text: '我去设置另一台电脑' }),
    Text({
      type: 'muted',
      text: state.otherComputer ? `已记下 ${state.otherComputer}` : '写下那台正在等待的电脑的地址。',
    }),
    elRow([
      Input({ id: 'other', placeholder: '地址' }),
      Button({
        text: '记下',
        onClick: () => page.commit(noteOtherComputer(page.state.data, page.field('other'))),
      }),
    ]),
    Text({ type: 'muted', text: '还没有要添加的应用。' }),
    elRow([
      Button({ text: '退出', onClick: () => page.commit(logout(page.state.data)) }),
      state.resetArmed
        ? Button({
            type: 'primary',
            text: '确认重置',
            onClick: () => page.resetDesk(),
          })
        : Button({ text: '重置', onClick: () => page.commit(armReset(page.state.data, true)) }),
    ]),
    state.resetArmed
      ? Text({
          type: 'danger',
          text: '重置会清掉这台电脑上记下的身份，以及这个浏览器里的文件、消息、Agent 和工作。退出只是离开，这些都还在。',
        })
      : null
  );
}

function formError(state) {
  return state.formError ? Text({ type: 'danger', text: state.formError }) : null;
}

function stack(...children) {
  return el('div', { className: 'stack' }, children);
}

function elRow(...children) {
  return el('div', { className: 'row' }, children);
}

function entry(title, detail, actions) {
  const buttons = actions == null ? [] : Array.isArray(actions) ? actions : [actions];
  return el(
    'div',
    { className: 'entry' },
    el(
      'div',
      { className: 'meta' },
      Text({ text: title }),
      detail ? Text({ type: 'secondary', text: detail }) : null
    ),
    buttons.length ? el('div', { className: 'row' }, buttons) : null
  );
}

WorkspacePage.prototype.ingest = function ingest(file) {
  const reader = new FileReader();
  reader.onload = () => {
    const text = typeof reader.result === 'string' ? reader.result : '';
    this.commit(addFile(this.state.data, file.name, text.slice(0, 20000)));
  };
  reader.onerror = () => {
    this.commit(addFile(this.state.data, file.name, '这个文件已放进来。这一版只能预览文本。'));
  };
  if (file.type.startsWith('text') || file.name.endsWith('.md') || file.name.endsWith('.txt')) {
    reader.readAsText(file);
  } else {
    this.commit(addFile(this.state.data, file.name, '这个文件已放进来。这一版只能预览文本。'));
  }
};
