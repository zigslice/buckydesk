import { ComponentRender } from '/gd5/src/engine/index.js';
import { ShellPage } from './pages/ShellPage.js';

const app = new ComponentRender();
app.init({ loading: 'shell', home: 'shell' }, { shell: ShellPage });

const boot = () => app.render();
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
