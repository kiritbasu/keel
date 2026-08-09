/**
 * Rendered markdown, for reading a whole spec in the app.
 *
 * `react-markdown` rather than a string-to-HTML library: it does not render
 * raw HTML by default, so a document body — which arrives from the store and
 * could have come from anywhere an agent wrote it — cannot inject markup. That
 * matters more here than it looks, because these bodies are written by a model
 * and read in an app with the daemon on the same origin.
 *
 * Every element is mapped explicitly rather than pulling in a typography
 * plugin. Twenty lines of class names against another dependency, on the same
 * reasoning as the rest of the components.
 */

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ReactNode } from "react";

export function Markdown({ children }: { children: string }) {
  return (
    <div className="selectable text-[14px] leading-relaxed">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: (p) => <h1 className="mt-8 mb-3 text-[22px] font-semibold tracking-tight" {...p} />,
          h2: (p) => (
            <h2
              className="mt-7 mb-2.5 border-b border-border-subtle pb-1.5 text-[17px] font-semibold tracking-tight"
              {...p}
            />
          ),
          h3: (p) => <h3 className="mt-5 mb-2 text-[15px] font-semibold" {...p} />,
          h4: (p) => <h4 className="mt-4 mb-1.5 text-[14px] font-semibold text-ink-muted" {...p} />,
          p: (p) => <p className="my-3" {...p} />,
          ul: (p) => <ul className="my-3 list-disc space-y-1 pl-5" {...p} />,
          ol: (p) => <ol className="my-3 list-decimal space-y-1 pl-5" {...p} />,
          li: (p) => <li className="pl-0.5" {...p} />,
          strong: (p) => <strong className="font-semibold text-ink" {...p} />,
          em: (p) => <em className="italic" {...p} />,
          a: (p) => (
            <a
              className="text-accent underline decoration-accent/40 underline-offset-2 hover:decoration-accent"
              target="_blank"
              rel="noreferrer"
              {...p}
            />
          ),
          blockquote: (p) => (
            <blockquote
              className="my-4 border-l-2 border-accent/40 bg-surface-hover/40 py-1 pl-4 text-ink-muted"
              {...p}
            />
          ),
          hr: () => <hr className="my-6 border-border-subtle" />,
          code: ({ className, children, ...rest }) => {
            // Fenced blocks arrive with a language class; inline code does not.
            const fenced = /language-/.test(className ?? "");
            if (!fenced) {
              return (
                <code
                  className="rounded bg-surface-hover px-1 py-0.5 font-mono text-[12.5px] text-ink"
                  {...rest}
                >
                  {children}
                </code>
              );
            }
            return (
              <code className="font-mono text-[12.5px] leading-relaxed" {...rest}>
                {children}
              </code>
            );
          },
          pre: (p) => (
            <pre
              className="my-4 overflow-x-auto rounded-lg border border-border-subtle bg-surface-hover/60 p-3"
              {...p}
            />
          ),
          // Tables carry a lot of these documents' meaning — the decision log
          // and the status tracker are almost entirely tables — so they get a
          // scroll container of their own rather than forcing the page wide.
          table: ({ children }: { children?: ReactNode }) => (
            <div className="my-4 overflow-x-auto rounded-lg border border-border-subtle">
              <table className="w-full border-collapse text-[13px]">{children}</table>
            </div>
          ),
          thead: (p) => <thead className="bg-surface-hover/60" {...p} />,
          th: (p) => (
            <th
              className="border-b border-border-subtle px-3 py-2 text-left font-semibold"
              {...p}
            />
          ),
          td: (p) => <td className="border-b border-border-subtle/60 px-3 py-2 align-top" {...p} />,
        }}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
