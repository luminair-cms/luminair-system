import { create } from 'zustand';

// ---------------------------------------------------------------------------
// Types & Interfaces
// ---------------------------------------------------------------------------

export interface DraftState {
  drafts: Record<string, Record<string, unknown>>;
  setDraft: (typeId: string, instanceId: string, data: Record<string, unknown>) => void;
  getDraft: (typeId: string, instanceId: string) => Record<string, unknown> | undefined;
  clearDraft: (typeId: string, instanceId: string) => void;
  clearAllDrafts: () => void;
}

// ---------------------------------------------------------------------------
// Primary Export (In-Memory Draft Store)
// ---------------------------------------------------------------------------

export const useDraftStore = create<DraftState>((set, get) => ({
  drafts: {},

  setDraft: (typeId, instanceId, data) => {
    const key = createDraftKey(typeId, instanceId);
    set((state) => ({
      drafts: {
        ...state.drafts,
        [key]: data,
      },
    }));
  },

  getDraft: (typeId, instanceId) => {
    const key = createDraftKey(typeId, instanceId);
    return get().drafts[key];
  },

  clearDraft: (typeId, instanceId) => {
    const key = createDraftKey(typeId, instanceId);
    set((state) => {
      const { [key]: _removed, ...remaining } = state.drafts;
      return { drafts: remaining };
    });
  },

  clearAllDrafts: () => {
    set({ drafts: {} });
  },
}));

// ---------------------------------------------------------------------------
// Internal Helpers
// ---------------------------------------------------------------------------

function createDraftKey(typeId: string, instanceId: string): string {
  return `${typeId}:${instanceId}`;
}
