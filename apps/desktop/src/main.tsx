/// <reference types="vite/client" />

import React from 'react';
import ReactDOM from 'react-dom/client';
import { createMnemeApiClient } from './api/client';
import { App } from './app/App';
import './design-system/tokens.css';
import './styles.css';

// 开发期默认连接本地 daemon；后续 Tauri 阶段可以改成由桌面壳注入。
const client = createMnemeApiClient(import.meta.env.VITE_MNEME_API_BASE_URL ?? 'http://127.0.0.1:3766');

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App client={client} />
  </React.StrictMode>,
);
