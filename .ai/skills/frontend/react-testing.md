# Skill: Frontend Testing with Vitest, RTL & MSW

Load this skill when writing, running, or reviewing frontend unit and component tests in `frontend/`.
Reference architecture: [`docs/ui-architecture.md`](../../../docs/ui-architecture.md).

---

## 1. Testing Stack & Setup

- **Test Runner**: Vitest (`vitest`) with jsdom environment
- **Component Testing**: React Testing Library (`@testing-library/react`)
- **User Interactions**: `@testing-library/user-event`
- **Network Mocking**: Mock Service Worker (`msw`)

---

## 2. Test Providers Wrapper

Always render components wrapped with required providers (`MantineProvider`, `QueryClientProvider`):

```typescript
// frontend/src/test-utils/render.tsx
import { render as rtlRender } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import React, { ReactElement } from 'react';

export function renderWithProviders(ui: ReactElement) {
  const testQueryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });

  return rtlRender(
    <QueryClientProvider client={testQueryClient}>
      <MantineProvider>{ui}</MantineProvider>
    </QueryClientProvider>
  );
}
```

---

## 3. MSW Handlers for RFC 9457 Responses

Mock API endpoints by returning canonical problem details:

```typescript
// frontend/src/test-utils/server-handlers.ts
import { http, HttpResponse } from 'msw';

export const handlers = [
  http.get('/api/v1/schema/document-types', () => {
    return HttpResponse.json({
      data: [{ id: 'article', name: 'Article', version: 1 }],
    });
  }),

  // Conflict 409 simulation
  http.post('/api/v1/content/:type/:id/publish', () => {
    return HttpResponse.json(
      {
        type: 'urn:luminair:error:draft_has_pending_edits',
        title: 'Conflict',
        status: 409,
        code: 'draft_has_pending_edits',
        detail: 'Pending edits conflict with target revision',
        details: { draft_version: 3 },
      },
      { status: 409, headers: { 'Content-Type': 'application/problem+json' } }
    );
  }),
];
```

---

## 4. Component Testing Recipe

```typescript
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect } from 'vitest';
import { renderWithProviders } from '@/test-utils/render';
import { CreateRoleForm } from '@/components/roles/CreateRoleForm';

describe('CreateRoleForm', () => {
  it('validates required fields before submitting', async () => {
    const user = userEvent.setup();
    const handleSubmit = vi.fn();

    renderWithProviders(<CreateRoleForm onSubmit={handleSubmit} />);

    const submitBtn = screen.getByRole('button', { name: /create role/i });
    await user.click(submitBtn);

    expect(await screen.findByText(/name is required/i)).toBeInTheDocument();
    expect(handleSubmit).not.toHaveBeenCalled();
  });

  it('submits valid input successfully', async () => {
    const user = userEvent.setup();
    const handleSubmit = vi.fn();

    renderWithProviders(<CreateRoleForm onSubmit={handleSubmit} />);

    await user.type(screen.getByLabelText(/role name/i), 'Editor');
    await user.type(screen.getByLabelText(/description/i), 'Can edit articles');
    await user.click(screen.getByRole('button', { name: /create role/i }));

    expect(handleSubmit).toHaveBeenCalledWith({
      name: 'Editor',
      description: 'Can edit articles',
    });
  });
});
```

---

## 5. Testing Custom Query Hooks

```typescript
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { createWrapper } from '@/test-utils/render';
import { useDocumentType } from '@/api/schema.api';

describe('useDocumentType', () => {
  it('fetches and returns document type', async () => {
    const { result } = renderHook(() => useDocumentType('article'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.name).toBe('Article');
  });
});
```
