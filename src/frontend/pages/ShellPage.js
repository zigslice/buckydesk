import { Component } from '/gd5/src/engine/index.js';
import { Button, Card, Text, Title, loadUi } from '/gd5/src/ui/index.js';

export class ShellPage extends Component {
  static styles = new URL('./ShellPage.css', import.meta.url);

  async componentDidMount() {
    try {
      await loadUi();
      const response = await fetch('/api/commands');
      if (!response.ok) {
        throw new Error(`无法读取命令树（${response.status}）`);
      }
      const tree = await response.json();
      this.setData({ tree, result: null, error: '', running: '' }, { refresh: false });
      this.refreshStatus();
    } catch (error) {
      this.setData(
        {
          tree: null,
          result: null,
          running: '',
          error: error instanceof Error ? error.message : String(error),
        },
        { refresh: false }
      );
    }
  }

  async refreshStatus() {
    try {
      const response = await fetch('/api/exec', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ argv: ['kernel', 'status'] }),
      });
      const result = await response.json();
      this.setData({ kernelStatus: result }, { refresh: false });
    } catch (e) {
      console.error('Failed to get kernel status:', e);
    }
  }

  async run(argv) {
    this.setData({ running: argv.join(' '), error: '' });
    try {
      const response = await fetch('/api/exec', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ argv }),
      });
      const result = await response.json();
      this.setData({ running: '', result, error: '' });
      this.ids().result?.scrollIntoView({ block: 'nearest' });
      if (argv[0] === 'kernel' || argv[0] === 'identity') {
        this.refreshStatus();
      }
    } catch (error) {
      this.setData({
        running: '',
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  render() {
    const data = this.state.data || {};
    const tree = data.tree;
    const kernelStatus = data.kernelStatus;
    this.paint(({ div }) => {
      div(
        { id: 'root', className: 'shell' },
        Title({ level: 1, text: 'BuckyDesk' }),
        Text({
          type: 'secondary',
          text: tree?.summary || '本机 BuckyOS 无界面内核的命令壳',
        }),
        kernelStatus ? statusCard(kernelStatus) : null,
        data.error ? Text({ type: 'danger', text: data.error }) : null,
        data.result ? resultCard(data.result) : null,
        tree ? nodes(tree.commands || [], this) : null
      );
    });
  }
}

function statusCard(result) {
  if (!result.ok || !result.data) return null;
  const { state, zone_id, gateway_available, provision_status } = result.data;
  
  const stateMap = {
    running: { text: '运行中', type: 'success' },
    stopped: { text: '已停止', type: 'warning' },
    unactivated: { text: '待激活', type: 'warning' },
    not_installed: { text: '未安装', type: 'danger' },
    starting: { text: '启动中', type: 'default' },
    error: { text: '错误', type: 'danger' },
  };
  
  const stateInfo = stateMap[state] || { text: state, type: 'default' };
  
  return Card(
    { title: '系统状态' },
    Text({ type: stateInfo.type, text: `内核状态: ${stateInfo.text}` }),
    zone_id ? Text({ type: 'secondary', text: `Zone: ${zone_id}` }) : null,
    Text({ 
      type: gateway_available ? 'success' : 'muted', 
      small: true, 
      text: `网关: ${gateway_available ? '可用' : '不可用'}` 
    })
  );
}

function nodes(list, page) {
  return list.map((node) => commandCard(node, page));
}

function commandCard(node, page) {
  const path = node.path || [node.name];
  const line = ['buckydesk', ...path].join(' ');
  const children = node.commands || [];
  const runnable = children.length === 0 && node.surface !== 'cli';
  const busy = page.state.data.running === path.join(' ');
  const flags = (node.args || [])
    .map((arg) => (arg.takesValue ? `--${arg.name} <值>` : `--${arg.name}`))
    .join('  ');
  return Card(
    { title: node.name },
    Text({ type: 'secondary', text: node.summary }),
    Text({ as: 'span', code: true, small: true, text: line }),
    flags ? Text({ type: 'muted', small: true, text: flags }) : null,
    runnable
      ? Button({
          type: 'primary',
          text: '运行',
          loading: busy,
          onClick: () => page.run(path),
        })
      : null,
    children.length ? nodes(children, page) : null
  );
}

function resultCard(result) {
  const data = result.data && Object.keys(result.data).length 
    ? JSON.stringify(result.data, null, 2) 
    : '';
  return Card(
    { id: 'result', title: result.ok ? '结果' : '未完成' },
    Text({ type: result.ok ? 'default' : 'danger', text: result.summary || '' }),
    data ? Text({ as: 'pre', code: true, small: true, text: data }) : null
  );
}
