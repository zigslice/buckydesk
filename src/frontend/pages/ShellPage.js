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
    this.paint(({ div }) => {
      div(
        { id: 'root', className: 'shell' },
        Title({ level: 1, text: 'BuckyDesk' }),
        Text({
          type: 'secondary',
          text: tree?.summary || '本机命令壳。命令行和这个页面共用同一棵树。',
        }),
        data.error ? Text({ type: 'danger', text: data.error }) : null,
        tree ? nodes(tree.commands || [], this) : null,
        data.result ? resultCard(data.result) : null
      );
    });
  }
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
    Text({ as: 'p', code: true, small: true, text: line }),
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
  const data = result.data && Object.keys(result.data).length ? JSON.stringify(result.data) : '';
  return Card(
    { title: result.ok ? '结果' : '未完成' },
    Text({ type: result.ok ? 'default' : 'danger', text: result.summary || '' }),
    data ? Text({ as: 'p', code: true, small: true, text: data }) : null
  );
}
