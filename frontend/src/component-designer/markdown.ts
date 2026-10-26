const escapeHtml = (value: string): string =>
  value.replace(
    /[&<>"']/g,
    (character) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[character] ?? character,
  );

function safeLink(url: string): string | undefined {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" || parsed.protocol === "http:"
      ? parsed.href
      : undefined;
  } catch {
    return undefined;
  }
}

function renderInline(source: string): string {
  const placeholders: string[] = [];
  const stash = (html: string): string => {
    placeholders.push(html);
    return `\uE000${placeholders.length - 1}\uE001`;
  };
  let rendered = source.replace(/`([^`]+)`/g, (_, code: string) =>
    stash(`<code>${escapeHtml(code)}</code>`),
  );
  rendered = rendered.replace(
    /\[([^\]]+)\]\(([^\s)]+)\)/g,
    (match, label: string, target: string) => {
      const href = safeLink(target);
      return href
        ? stash(
            `<a href="${escapeHtml(href)}" target="_blank" rel="noopener noreferrer">${escapeHtml(label)}</a>`,
          )
        : escapeHtml(match);
    },
  );
  rendered = escapeHtml(rendered)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|\s)\*([^*]+)\*/g, "$1<em>$2</em>");
  return rendered.replace(
    /\uE000(\d+)\uE001/g,
    (_, index: string) => placeholders[Number(index)] ?? "",
  );
}

/** Renders a deliberately small Markdown subset without accepting raw HTML or active URLs. */
export function renderSanitizedMarkdown(markdown: string): string {
  const output: string[] = [];
  const paragraph: string[] = [];
  let inCode = false;
  let code: string[] = [];
  let inList = false;
  const flushParagraph = (): void => {
    if (paragraph.length > 0) {
      output.push(`<p>${renderInline(paragraph.join(" "))}</p>`);
      paragraph.length = 0;
    }
  };
  const closeList = (): void => {
    if (inList) output.push("</ul>");
    inList = false;
  };

  for (const line of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    if (line.startsWith("```")) {
      if (inCode) {
        output.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
        code = [];
        inCode = false;
      } else {
        flushParagraph();
        closeList();
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      code.push(line);
      continue;
    }
    if (!line.trim()) {
      flushParagraph();
      closeList();
      continue;
    }
    const heading = /^(#{1,4})\s+(.+)$/.exec(line);
    if (heading) {
      flushParagraph();
      closeList();
      const level = heading[1]!.length;
      output.push(`<h${level}>${renderInline(heading[2]!)}</h${level}>`);
      continue;
    }
    const item = /^[-*]\s+(.+)$/.exec(line);
    if (item) {
      flushParagraph();
      if (!inList) {
        output.push("<ul>");
        inList = true;
      }
      output.push(`<li>${renderInline(item[1]!)}</li>`);
      continue;
    }
    closeList();
    paragraph.push(line.trim());
  }
  if (inCode)
    output.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
  flushParagraph();
  closeList();
  return output.join("\n");
}
