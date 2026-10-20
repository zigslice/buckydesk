import { ComponentRender } from '/gd5/src/engine/index.js';
import { WorkspacePage } from './pages/WorkspacePage.js';

const app = new ComponentRender();
app.init({ loading: 'shell', home: 'shell' }, { shell: WorkspacePage });

const boot = () => app.render();
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot);
} else {
  boot();
}
