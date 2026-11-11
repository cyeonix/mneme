/// <reference types="vite/client" />

import React from 'react';
import ReactDOM from 'react-dom/client';
import { createMnemeApiClient } from './api/client';
import { App } from './app/App';
import './design-system/tokens.css';
import './styles.css';
import { getApiBaseUrl } from './tauri/bridge';

async function bootstrap(): Promise<void> {
  const apiBaseUrl = await getApiBaseUrl();
  const client = createMnemeApiClient(apiBaseUrl);

  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <App client={client} />
    </React.StrictMode>,
  );
}

void bootstrap();
