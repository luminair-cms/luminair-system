# Skill: React & Frontend Design Patterns (Luminair Admin)

Load this skill before designing, implementing, refactoring, or reviewing frontend code in `frontend/`.
Reference architecture: [`docs/ui-architecture.md`](../../../docs/ui-architecture.md).

---

## 1. Stack & Architecture Overview

The Luminair Admin Dashboard is a React 19.3 Single Page Application built with Vite 8 and TypeScript 7:
- **Design System**: Mantine v9 (`@mantine/core`, `@mantine/hooks`, `@mantine/form`, `@mantine/notifications`, `@mantine/modals`, `@mantine/dates`, `@mantine/tiptap`)
- **Server State**: TanStack Query v5 (`@tanstack/react-query`)
- **Client State**: Zustand v5 (`zustand`)
- **Routing**: TanStack Router v1 (`@tanstack/react-router`)
- **HTTP Client**: Axios (`axios`)
- **Validation**: Zod v4 (`zod`)
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

## 4. Mantine Form & Dynamic Schema Rendering

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

---

## 7. React 19.0 / 19.2 / 19.3 Patterns & Stack Harmonization

### 7.1 `ref` as a Prop & Direct `<Context>` Provider (React 19.0)
Never use `React.forwardRef` or `<Context.Provider>`. Pass `ref` directly as a prop and render `<Context value={...}>` directly:

```tsx
import { createContext, use, type ReactElement, type Ref } from 'react';
import { TextInput, type TextInputProps } from '@mantine/core';

export const SchemaTypeContext = createContext<string | null>(null);

export interface CustomTextControlProps extends TextInputProps {
  ref?: Ref<HTMLInputElement>;
}

export function CustomTextControl({ ref, ...props }: CustomTextControlProps): ReactElement {
  // `use()` can be called conditionally after early returns
  const schemaType = use(SchemaTypeContext);
  return <TextInput ref={ref} description={schemaType ?? undefined} {...props} />;
}
```

### 7.2 State Preservation with `<Activity>` (React 19.2)
When switching between multi-locale tabs (`LocalizedTextTabs`), `Working Draft` vs `Published Live` editor tabs, or collapsible side panels (`SnapshotHistoryPanel`), wrap tab panes in `<Activity mode={isActive ? 'visible' : 'hidden'}>` rather than unmounting them (`{isActive && <Pane />}`). This preserves DOM state, cursor selection, and unsaved edits while unmounting effects while hidden:

```tsx
import { Activity, type ReactElement } from 'react';
import { Textarea } from '@mantine/core';

export interface LocalizedFieldProps {
  locales: string[];
  activeLocale: string;
  values: Record<string, string>;
  onChange: (locale: string, val: string) => void;
}

export function LocalizedFieldPanes({
  locales,
  activeLocale,
  values,
  onChange,
}: LocalizedFieldProps): ReactElement {
  return (
    <>
      {locales.map((locale) => (
        <Activity key={locale} mode={activeLocale === locale ? 'visible' : 'hidden'}>
          <Textarea
            label={`Content (${locale.toUpperCase()})`}
            value={values[locale] ?? ''}
            onChange={(e) => onChange(locale, e.currentTarget.value)}
          />
        </Activity>
      ))}
    </>
  );
}
```

### 7.3 Non-Reactive Effect Callbacks with `useEffectEvent` (React 19.2)
When an effect (such as an auto-save timer, OIDC session event listener, or WebSocket/polling handler) needs to read the latest state or props without re-subscribing every time those values change, extract the callback with `useEffectEvent`:

```tsx
import { useEffect, useEffectEvent } from 'react';

export function useDraftAutoSync(
  draftPayload: Record<string, unknown>,
  enabled: boolean,
  onSync: (payload: Record<string, unknown>) => void
): void {
  const triggerSync = useEffectEvent(() => {
    onSync(draftPayload);
  });

  useEffect(() => {
    if (!enabled) return;
    const id = window.setInterval(() => triggerSync(), 15_000);
    return () => window.clearInterval(id);
  }, [enabled]);
}
```

### 7.4 Native Document Metadata (`<title>`) & `<ViewTransition>` (React 19.0 / 19.3)
- Render `<title>` directly inside route components (React hoists it to `<head>` automatically).
- Use `useTransition` / `startTransition` with `addTransitionType` and `<ViewTransition>` when animating master-detail or tab transitions, and `<Fragment ref={...}>` (`FragmentInstance`) when managing focus or `ResizeObserver` across multi-element field controls without extra wrapper `<div>`s.


---

## 8. React Compiler 1.0 (Automatic Memoization)

Rules: [`.ai/rules/frontend-react.md` §4.1](../../rules/frontend-react.md). The compiler runs in `infer` mode for all of `frontend/src`; write plain idiomatic code and let it memoize.

### 8.1 Build & Lint Wiring (Vite 8 + `@vitejs/plugin-react` 6)

`@vitejs/plugin-react` v6 removed inline Babel, so the compiler is attached via `@rolldown/plugin-babel` and the exported `reactCompilerPreset` (`vite.config.ts`):

```typescript
import { defineConfig } from 'vite';
import react, { reactCompilerPreset } from '@vitejs/plugin-react';
import babel from '@rolldown/plugin-babel';

export default defineConfig({
  plugins: [
    react(),
    // Compiler first so it sees original source; REACT_COMPILER=off isolates compiler bugs.
    ...(process.env.REACT_COMPILER === 'off'
      ? []
      : [babel({ presets: [reactCompilerPreset()] })]),
  ],
});
```

If compiler options are needed (e.g. `panicThreshold: 'none'`, a `logger`), use the explicit form instead of the preset: `babel({ plugins: [['babel-plugin-react-compiler', { ... }]] })`.

ESLint (flat config) — compiler diagnostics come from `eslint-plugin-react-hooks`, no separate `eslint-plugin-react-compiler`:

```javascript
import reactHooks from 'eslint-plugin-react-hooks';
// ...inside the exported config array:
reactHooks.configs.flat.recommended,
```

### 8.2 Write Compiler-Friendly Code

```tsx
// ❌ Before: manual memoization noise
const rows = useMemo(() => toRows(headers), [headers]);
const onSelect = useCallback((id: string) => select(id), [select]);

// ✅ After: plain code, the compiler memoizes (including after early returns)
export function HeadersTable({ headers, select }: HeadersTableProps): ReactElement {
  if (headers.length === 0) return <EmptyState />;
  const rows = toRows(headers);
  return <DataTable data={rows} onRowClick={(row) => select(row.id)} />;
}
```

| Pattern | Verdict | Do instead |
|---|---|---|
| `useMemo` / `useCallback` / `memo()` for speed | ❌ | Remove; rely on the compiler. Keep only as a commented escape hatch (effect dependency identity) |
| `Date.now()`, `Math.random()`, `crypto.randomUUID()`, `getState()` in render | ❌ impure | Generate in event handler/effect; read stores via selector hooks |
| `useEffect(() => form.setValues(doc), [doc])` (copy server data into state) | ❌ `set-state-in-effect` | `<DynamicForm key={doc.id + doc.audit.version} initialValues={toFormValues(doc)} />` |
| Mutating `props`, state arrays/objects, or hook results | ❌ | Create new values (`toSorted`, spread, `structuredClone`) |
| Reading/writing `ref.current` during render | ❌ `refs` | Touch refs in handlers, effects, or ref callbacks |
| Effect keyed on an object/array identity | ❌ memoization-for-correctness | Depend on primitives (`doc.id`, `audit.version`) or use `useEffectEvent` |
| Module-level pure helper for a heavy computation shared by many components | ✅ | Cache inside the helper only if profiling proves it is needed |

### 8.3 Opting Out (Temporary Only)

```tsx
function GridWithDynamicRowHeights(): ReactElement {
  "use no memo"; // TODO(UI-123): remove once <cause> is fixed — compiler skipped due to <reason>
  // ...
}
```

Debugging order: reproduce with `REACT_COMPILER=off` → add `"use no memo"` to the suspect component → if the bug persists after removing all manual memoization it is a Rules-of-React violation (fix the code), otherwise file a minimal repro with React + compiler versions. Verify success via the `Memo ✨` badge in React DevTools.

### 8.4 Interaction With the Rest of the Stack

- **TanStack Query / Router, Mantine (`@mantine/form`, `@tiptap`), Zustand, i18next**: not assumed compatible — the compiler silently skips components it cannot prove safe, and libraries that hand out mutable/identity-unstable objects can surface stale UI. Compatibility is verified empirically (compiler `logger` skip report + the suite in `src/**/__tests__`) in plan Subtask 1.4 and re-checked whenever a library is upgraded.
- **React 19.2/19.3 primitives** (`<Activity>`, `useEffectEvent`, `<ViewTransition>`, `use()`): used as before; the compiler handles `use()` after conditional returns. `useEffectEvent` callbacks must be called only from effects (enforced by the hooks lint rules).
- **Tests** are expected to run through the same Vite plugin pipeline (Vitest reads `vite.config.ts`), so component tests exercise compiled output — to be confirmed in plan Subtask 1.4. Run `REACT_COMPILER=off pnpm test` when a failure might be compiler related.
