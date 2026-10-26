import { describe, expect, it } from "vitest";
import { renderSanitizedMarkdown } from "./markdown";

describe("component documentation Markdown", () => {
  it("renders useful reference content", () => {
    const html = renderSanitizedMarkdown(
      "# Pinout\n\n- **VCC** supply\n- See [datasheet](https://example.com/part.pdf)",
    );
    expect(html).toContain("<h1>Pinout</h1>");
    expect(html).toContain("<strong>VCC</strong>");
    expect(html).toContain('href="https://example.com/part.pdf"');
    expect(html).toContain('rel="noopener noreferrer"');
  });

  it("escapes raw HTML and refuses active link protocols", () => {
    const html = renderSanitizedMarkdown(
      "<img src=x onerror=alert(1)> [run](javascript:alert(1)) <script>alert(2)</script>",
    );
    expect(html).not.toContain("<img");
    expect(html).not.toContain("<script");
    expect(html).not.toContain('href="javascript:');
    expect(html).toContain("&lt;img");
    expect(html).toContain("javascript:alert(1)");
  });

  it("escapes fenced code instead of interpreting it", () => {
    const html = renderSanitizedMarkdown(
      "```html\n<button onclick=x>Go</button>\n```",
    );
    expect(html).toContain("<pre><code>");
    expect(html).toContain("&lt;button onclick=x&gt;");
  });
});
