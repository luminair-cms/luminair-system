# Skill: React & Frontend Design Patterns (Luminair Admin)

Load this skill before designing, implementing, refactoring, or reviewing frontend code in `frontend/`.
Reference architecture: [`docs/ui-architecture.md`](../../../docs/ui-architecture.md).

---

## 1. Stack & Architecture Overview

The Luminair Admin Dashboard is a React 19 Single Page Application built with Vite and TypeScript:
- **Design System**: Mantine v7 (`@mantine/core`, `@mantine/hooks`, `@mantine/form`, `@mantine/notifications`, `@mantine/modals`)
- **Server State**: TanStack Query v5 (`@tanstack/react-query`)
- **Client State**: Zustand (`zustand`)
- **Routing**: TanStack Router (`@tanstack/react-router`)
- **HTTP Client**: Axios (`axios`)
- **Validation**: Zod (`zod`)
- **Error Standard**: RFC 9457 `application/problem+json`

---

## 2. Server State with TanStack Query v5

### Centralized Query Key Factory (`api/query-keys.ts`)

Never construct inline string query keys. Use hierarchical tuple factories:

```typescript
export const queryKeys = {
  schema: {
    all: ['schema'] as const,
    documentTypes: () => [...queryKeys.schema.all, 'document-types'] as const,
    documentType: (id: string) => [...queryKeys.schema.documentTypes(), id] as const,
  },
  documents: {
    all: ['documents'] as const,
    list: (type: string, filters: Record<string, unknown>) =>
      [...queryKeys.documents.all, type, 'list', filters] as const,
    detail: (type: string, id: string) =>
      [...queryKeys.documents.all, type, 'detail', id] as const,
    snapshots: (type: string, id: string) =>
      [...queryKeys.documents.detail(type, id), 'snapshots'] as const,
  },
  system: {
    config: () => ['system', 'config'] as const,
    health: () => ['system', 'health'] as const,
  },
} as const;
```

### Custom Query & Mutation Hooks (`api/*.api.ts`)

Encapsulate all data fetching and mutations inside focused custom hooks:

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { apiClient } from './client';
import { queryKeys } from './query-keys';

export function useDocumentType(typeId: string) {
  return useQuery({
    queryKey: queryKeys.schema.documentType(typeId),
    queryFn: async () => {
      const response = await apiClient.get(`/api/v1/schema/document-types/${typeId}`);
      return response.data;
    },
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}

export function usePublishDocument(type: string, id: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (expectedVersion: number) => {
      const response = await apiClient.post(
        `/api/v1/content/${type}/${id}/publish`,
        { expectedVersion }
      );
      return response.data;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.documents.detail(type, id) });
      queryClient.invalidateQueries({ queryKey: queryKeys.documents.all });
    },
  });
}
```

---

## 3. Client State with Zustand

Use Zustand strictly for cross-cutting client state (e.g. auth tokens, draft editing buffers, sidebar state):

```typescript
import { create } from 'zustand';

interface AuthState {
  token: string | null;
  user: { id: string; email: string; roles: string[] } | null;
  setSession: (token: string, user: AuthState['user']) => void;
  clearSession: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  token: null,
  user: null,
  setSession: (token, user) => set({ token, user }),
  clearSession: () => set({ token: null, user: null }),
}));
```

---

## 4. Mantine v7 Form & Dynamic Schema Rendering

### Static Form Handling with `@mantine/form`

```typescript
import { useForm } from '@mantine/form';
import { TextInput, Button, Stack } from '@mantine/core';

export function CreateRoleForm({ onSubmit }: { onSubmit: (values: RoleFormValues) => void }) {
  const form = useForm({
    initialValues: { name: '', description: '' },
    validate: {
      name: (val) => (val.trim().length === 0 ? 'Name is required' : null),
    },
  });

  return (
    <form onSubmit={form.onSubmit(onSubmit)}>
      <Stack gap="md">
        <TextInput label="Role Name" required {...form.getInputProps('name')} />
        <TextInput label="Description" {...form.getInputProps('description')} />
        <Button type="submit">Create Role</Button>
      </Stack>
    </form>
  );
}
```

### Dynamic Form Mapping from Schema

Map schema field types (`String`, `Number`, `Boolean`, `DateTime`, `RichText`, `Relation`) to Mantine controls:
- `String`: `<TextInput>` or `<Textarea>`
- `Number`: `<NumberInput>` (supports decimal bounds)
- `Boolean`: `<Switch>` or `<Checkbox>`
- `DateTime`: `<DateTimePicker>` from `@mantine/dates`
- `RichText`: `<RichTextEditor>` from `@mantine/tiptap`
- `Relation`: Searchable `<Select>` or modal picker

---

## 5. RFC 9457 Error Handling & Localization

The Axios interceptor translates HTTP errors into `ProblemDetails`. Components map the machine-readable `error.code` to user copy:

```typescript
import { notifications } from '@mantine/notifications';
import { IconAlertCircle } from '@tabler/icons-react';
import { ProblemDetails } from '@/api/errors';

export function handleApiError(error: ProblemDetails, t: (key: string, params?: unknown) => string) {
  // If validation errors exist, let form handle field-level highlights
  if (error.status === 422 && error.invalid_params) {
    return;
  }

  // Map machine-readable code to translation dictionary
  const localizedMessage = t(`errors.${error.code}`, error.details) || error.detail;

  notifications.show({
    title: error.title || 'Error',
    message: localizedMessage,
    color: 'red',
    icon: <IconAlertCircle size={18} />,
  });
}
```

---

## 6. Type-Safe Routing with TanStack Router

Validate URL query parameters using Zod schemas:

```typescript
import { createFileRoute } from '@tanstack/react-router';
import { z } from 'zod';

const documentSearchSchema = z.object({
  page: z.number().int().positive().default(1),
  limit: z.number().int().positive().max(100).default(20),
  search: z.string().optional(),
  status: z.enum(['draft', 'published', 'archived']).optional(),
});

export const Route = createFileRoute('/content/$documentType/')({
  validateSearch: (search) => documentSearchSchema.parse(search),
  component: DocumentListPage,
});
```
