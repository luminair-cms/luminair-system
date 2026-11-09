# Frontend (React 19.3 + TypeScript 7) Rules

These non-negotiable rules and invariants govern all frontend code in `frontend/`, based on [`docs/ui-architecture.md`](../../docs/ui-architecture.md) and [`docs/api.md`](../../docs/api.md).

---

## 1. Strict TypeScript & Type Safety

- **Zero `any`**: The use of `any` is strictly prohibited. Use `unknown` with type guards or Zod runtime schema parsing.
- **Explicit Component Types**: All component props and custom hook return types (`ReactElement`, `UseQueryResult<...>`, etc.) must be explicitly typed using interfaces or type aliases.
- **Strict Compiler Compliance**: All code must pass `pnpm exec tsc --noEmit` (TypeScript 7.0+) and ESLint (`pnpm run lint`, including the React Compiler rules from `eslint-plugin-react-hooks`) checks with zero errors or warnings.

---

## 2. Mantine v9 Design System Invariants

- **Standard UI Primitives**: All UI components must use Mantine primitives (`@mantine/core`, `@mantine/hooks`, `@mantine/form`, `@mantine/notifications`, `@mantine/modals`, `@mantine/dates`, `@mantine/tiptap`).
- **Zero Raw CSS / Inline Styles**:
  - Do not write raw CSS files or inline `style={{ ... }}` objects when Mantine style props or theme tokens exist.
  - Colors, spacing, radii, typography, and dark/light mode adaptations must reference the centralized `theme.ts`.
- **System Notifications**: Display toasts and system alerts exclusively through `@mantine/notifications` (`notifications.show(...)`).

---

## 3. Server State vs. Client State

- **Server State via TanStack Query v5**:
  - All data fetching, caching, and mutations must go through TanStack Query v5 custom hooks located in `api/*.api.ts`.
  - **Never use `useEffect` or raw `use(fetchPromise)` for HTTP data fetching or mutation orchestration.**
- **Centralized Query Keys**:
  - Query keys must be defined as tuple factory functions in `api/query-keys.ts` (e.g. `queryKeys.documents.detail(type, id)`). Never construct raw query key strings inline in components.
- **Client State via Zustand**:
  - Global client state (auth tokens, current user claims, layout state, draft editing buffers) must live in dedicated Zustand stores (`useAuthStore`, `useUiStore`, `useDraftStore`).
  - Local component state is managed with React `useState` or Mantine form state (`@mantine/form`).

---

## 4. React 19.0 / 19.2 / 19.3 Modern Primitives & Harmonization

- **`ref` as a Regular Prop (No `forwardRef`)**:
  - `React.forwardRef` is forbidden. Pass `ref?: React.Ref<HTMLElement>` directly as a prop in function components.
  - Attach DOM observers or imperative event listeners using ref cleanup functions: `ref={(el) => { ...; return () => cleanup(); }}`.
- **Direct `<Context>` Provider & `use(Context)`**:
  - Render `<MyContext value={value}>` directly (never `<MyContext.Provider>`).
  - Read contexts via `use(MyContext)` to allow conditional access after early returns.
- **`<Activity>` State Preservation (React 19.2+)**:
  - Use `<Activity mode={isActive ? 'visible' : 'hidden'}>` instead of conditional unmounting (`{isActive && <Child />}`) whenever a hidden view holds unsaved form state, editor instances, or scroll positions.
- **`useEffectEvent` for Non-Reactive Callbacks (React 19.2+)**:
  - Wrap non-reactive callbacks executed inside `useEffect` (timers, event subscriptions, analytics) in `useEffectEvent` instead of suppressing ESLint dependency rules.
- **Native Document Metadata (`<title>`, `<meta>`)**:
  - Render `<title>` and `<meta>` tags directly in route components; never mutate `document.title` inside `useEffect`.
- **Concurrent Transitions, `<ViewTransition>` & `useOptimistic` (React 19.0–19.3)**:
  - Keep search, filter, and action transitions non-blocking via `useTransition`, `useOptimistic`, and `useDeferredValue(value, initialValue)`.
  - Wrap route/view switches in `<ViewTransition>` and use `<Fragment ref={...}>` (`FragmentInstance`) when measuring or focusing multi-node compound fields without wrapper elements.
- **Client SPA Scope**:
  - Do not use SSR/RSC-only primitives (`cacheSignal`, PPR `prerender`, or `use(browser())`) in this Vite client SPA.

### 4.1 React Compiler 1.0 (Automatic Memoization)

Build-time memoization is active for all of `frontend/src`. Code must strictly obey the Rules of React to enable automatic optimization:

- **No Manual Memoization for Performance**:
  - Do not use `useMemo`, `useCallback`, or `React.memo` to optimize render performance. Rely on the compiler.
  - `useMemo` / `useCallback` are permitted strictly as a documented escape hatch when referential identity is required by an external API or an effect dependency array.
- **Strict Render Purity & Immutability**:
  - Render bodies must be pure, deterministic, and idempotent. Never invoke non-deterministic functions or read store snapshots (`store.getState()`) during render; read stores exclusively via selector hooks.
  - Props, state, and hook return values are immutable. Never mutate them or read/write `ref.current` during render.
- **No Prop/Server State Mirroring**:
  - Never copy props or server state into local state inside `useEffect` (`set-state-in-effect`). Initialize state via component `key` or derive values during render.
- **Zero Compiler Diagnostic Suppressions**:
  - Never suppress React Compiler ESLint diagnostics with `eslint-disable`. Every diagnostic must be resolved by fixing the code.
- **Restricted `"use no memo"` Directive**:
  - Permitted strictly as a temporary, function-level escape hatch with a mandatory issue tracking comment: `"use no memo"; // TODO(issue): reason`. Module-level opt-out is prohibited.

*(See [`react-patterns.md` §8](../skills/frontend/react-patterns.md#8-react-compiler-10-automatic-memoization) for compiler build setup, the anti-pattern conversion table, and debugging workflows).*

---

## 5. Routing & Search Parameters

- **Type-Safe Routing**: Use TanStack Router for all application navigation and route definitions.
- **Search Parameter Validation**: All URL query parameters (filters, pagination, sort keys) must be validated with Zod schemas in route definitions via TanStack Router's `validateSearch`.

---

## 6. RFC 9457 Error Handling & Localization (i18n)

- **Problem Details Interception**:
  - The centralized Axios client (`api/client.ts`) must intercept all non-2xx responses and normalize them into RFC 9457 `ProblemDetails` structures.
- **Frontend Owns Localization**:
  - The UI client owns end-user localization. Always map `error.code` to the translation catalog: `t('errors.' + error.code, error.details)`.
  - **Never display the raw backend developer `detail` string directly to the user as localized copy.**
- **Form Field Errors**:
  - Endpoint validation errors (`422 Unprocessable Entity` containing `invalid_params`) must be mapped directly into `@mantine/form` field error states (`form.setErrors(...)`).

---

## 7. Component Architecture & Metrics

- **Component Size**:
  - Target **$\le 50$ lines** per component.
  - When a component exceeds 50 lines, decompose it by extracting sub-components, custom hooks, or utility helpers.
- **JSX Nesting & Readability**:
  - Maximum nesting depth of 2 levels for conditional branches or loops within JSX. Extract complex conditional branches into dedicated child components.
- **Single Responsibility**:
  - Presentational components focus solely on rendering UI and firing callbacks.
  - Custom hooks or container components encapsulate API calls, query mutations, and state management.

---

## 8. Newspaper Code Layout (TypeScript & React)

Every source file (`.ts`, `.tsx`) must follow top-down dependency ordering:
1. **Imports**: External libraries/frameworks (`react`, `@mantine/*`, `@tanstack/*`) → internal shared aliases (`@/api`, `@/components`) → local relative imports (`./helpers`)
2. **Types & Interfaces**: Component prop types, form values interfaces, and local state types
3. **Primary Export**: The main component, custom hook, or service entry point appears at the top
4. **Internal Helpers & Sub-Components**: Private leaf rendering functions, specialized child sub-components, and transformation helpers follow below
5. **Constants & Static Metadata**: Default values, static options, and style configs at the bottom
