// CodeMirror 6 wiring for the Rust submission editor.
//
// `mount` builds a fresh editor on the supplied host element. It returns
// a teardown function so the caller (a Svelte effect) can `destroy()` the
// view when the host unmounts.

import { rust } from '@codemirror/lang-rust';
import { syntaxHighlighting } from '@codemirror/language';
import { EditorState } from '@codemirror/state';
import { oneDarkHighlightStyle } from '@codemirror/theme-one-dark';
import { EditorView, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';

export interface MountOptions {
  initialDoc: string;
  onChange?: (doc: string) => void;
}

export function mount(host: HTMLElement, opts: MountOptions): () => void {
  const state = EditorState.create({
    doc: opts.initialDoc,
    extensions: [
      lineNumbers(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      rust(),
      syntaxHighlighting(oneDarkHighlightStyle),
      EditorView.theme(
        {
          '&': { height: '100%', fontSize: '13px' },
          '.cm-scroller': { fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace' },
          '.cm-gutters': { backgroundColor: 'transparent', borderRight: '1px solid #27272a' }
        },
        { dark: true }
      ),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && opts.onChange) {
          opts.onChange(update.state.doc.toString());
        }
      })
    ]
  });

  const view = new EditorView({ state, parent: host });
  return () => view.destroy();
}
