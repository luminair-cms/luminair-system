import { beforeEach, describe, expect, it } from 'vitest';
import { useDraftStore } from '../use-draft-store';

describe('useDraftStore', () => {
  beforeEach(() => {
    useDraftStore.getState().clearAllDrafts();
  });

  it('initializes with empty drafts buffer', () => {
    expect(useDraftStore.getState().drafts).toEqual({});
  });

  it('stores and retrieves draft by composite typeId:instanceId key', () => {
    const draftData = { title: 'Draft Post', content: 'In-progress content' };

    useDraftStore.getState().setDraft('articles', 'doc-1', draftData);

    const retrieved = useDraftStore.getState().getDraft('articles', 'doc-1');
    expect(retrieved).toEqual(draftData);
  });

  it('maintains independent drafts for different entities', () => {
    useDraftStore.getState().setDraft('articles', 'doc-1', { title: 'Article 1' });
    useDraftStore.getState().setDraft('articles', 'doc-2', { title: 'Article 2' });
    useDraftStore.getState().setDraft('categories', 'doc-1', { name: 'Category 1' });

    expect(useDraftStore.getState().getDraft('articles', 'doc-1')).toEqual({ title: 'Article 1' });
    expect(useDraftStore.getState().getDraft('articles', 'doc-2')).toEqual({ title: 'Article 2' });
    expect(useDraftStore.getState().getDraft('categories', 'doc-1')).toEqual({ name: 'Category 1' });
  });

  it('clears specific draft without affecting other drafts', () => {
    useDraftStore.getState().setDraft('articles', 'doc-1', { title: 'Article 1' });
    useDraftStore.getState().setDraft('articles', 'doc-2', { title: 'Article 2' });

    useDraftStore.getState().clearDraft('articles', 'doc-1');

    expect(useDraftStore.getState().getDraft('articles', 'doc-1')).toBeUndefined();
    expect(useDraftStore.getState().getDraft('articles', 'doc-2')).toEqual({ title: 'Article 2' });
  });

  it('clears all drafts simultaneously', () => {
    useDraftStore.getState().setDraft('articles', 'doc-1', { title: 'Article 1' });
    useDraftStore.getState().setDraft('articles', 'doc-2', { title: 'Article 2' });

    useDraftStore.getState().clearAllDrafts();

    expect(useDraftStore.getState().drafts).toEqual({});
  });
});
