import { setupServer } from 'msw/node';
import { handlers } from './server-handlers';

// ---------------------------------------------------------------------------
// Primary Export (Node MSW Server Instance)
// ---------------------------------------------------------------------------

export const server = setupServer(...handlers);
