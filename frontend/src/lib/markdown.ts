// Wrapper around `marked` configured for problem descriptions.
//
// Problems are admin-authored, so we trust the content and don't sanitize
// — but we disable raw HTML rendering as a belt-and-suspenders defense
// against a future world where non-admins can author markdown.

import { marked } from 'marked';

marked.use({
  gfm: true,
  breaks: false,
  renderer: {
    // Drop any raw HTML blocks/inlines from the source.
    html() {
      return '';
    }
  }
});

export function renderMarkdown(src: string): string {
  return marked.parse(src, { async: false }) as string;
}
