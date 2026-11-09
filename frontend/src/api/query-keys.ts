/**
 * Centralized TanStack Query key factory.
 *
 * Conforms to .ai/rules/frontend-react.md and docs/ui-architecture.md.
 * All query keys must be structured hierarchical tuples.
 */

// ---------------------------------------------------------------------------
// Primary Export (Query Key Factory)
// ---------------------------------------------------------------------------

export const queryKeys = {
  system: {
    all: ['system'] as const,
    config: () => [...queryKeys.system.all, 'config'] as const,
    locales: () => [...queryKeys.system.all, 'locales'] as const,
    health: () => [...queryKeys.system.all, 'health'] as const,
  },

  schema: {
    all: ['schema'] as const,
    documentTypes: () => [...queryKeys.schema.all, 'document-types'] as const,
    documentType: (id: string) => [...queryKeys.schema.documentTypes(), id] as const,
  },

  documents: {
    all: ['documents'] as const,
    list: (typeId: string, filters?: Record<string, unknown>) =>
      [...queryKeys.documents.all, typeId, 'list', filters ?? {}] as const,
    headers: (typeId: string, filters?: Record<string, unknown>) =>
      [...queryKeys.documents.all, typeId, 'headers', filters ?? {}] as const,
    detail: (typeId: string, id: string) =>
      [...queryKeys.documents.all, typeId, 'detail', id] as const,
    snapshots: (typeId: string, id: string) =>
      [...queryKeys.documents.detail(typeId, id), 'snapshots'] as const,
    snapshotDetail: (typeId: string, id: string, revision: number) =>
      [...queryKeys.documents.snapshots(typeId, id), revision] as const,
  },

  accessRequests: {
    all: ['access-requests'] as const,
    pending: () => [...queryKeys.accessRequests.all, 'pending'] as const,
    detail: (id: string) => [...queryKeys.accessRequests.all, 'detail', id] as const,
  },

  users: {
    all: ['users'] as const,
    list: (filters?: Record<string, unknown>) =>
      [...queryKeys.users.all, 'list', filters ?? {}] as const,
  },
} as const;
