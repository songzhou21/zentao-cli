package bug

import (
	"fmt"
	"net/url"
	"regexp"
	"strings"

	htmltomarkdown "github.com/JohannesKaufmann/html-to-markdown/v2"
	"github.com/PuerkitoBio/goquery"
)

type BugDetail struct {
	Title               string
	MarkdownDescription string
}

func ParseBugDetail(pageURL string, html string) (*BugDetail, error) {
	doc, err := goquery.NewDocumentFromReader(strings.NewReader(html))
	if err != nil {
		return nil, fmt.Errorf("解析 bug 页面失败: %w", err)
	}
	title := extractTitle(doc)
	if title == "" {
		return nil, fmt.Errorf("未解析到 bug 标题")
	}
	desc := extractDescriptionNode(doc)
	if desc == nil {
		return nil, fmt.Errorf("未解析到 bug 描述")
	}
	descHTML, err := desc.Html()
	if err != nil {
		return nil, fmt.Errorf("读取 bug 描述 HTML 失败: %w", err)
	}

	markdown, err := htmltomarkdown.ConvertString(descHTML)
	if err != nil {
		return nil, fmt.Errorf("描述 HTML 转 Markdown 失败: %w", err)
	}
	markdown, err = absolutizeMarkdownImageURLs(strings.TrimSpace(markdown), pageURL)
	if err != nil {
		return nil, err
	}
	attachments, err := extractAttachmentURLs(doc, pageURL)
	if err != nil {
		return nil, err
	}
	markdown = appendAttachmentLinks(markdown, attachments)
	markdown = normalizeMarkdown(markdown)

	return &BugDetail{Title: title, MarkdownDescription: markdown}, nil
}

func extractTitle(doc *goquery.Document) string {
	node := doc.Find("div.page-title span.text").First()
	if node.Length() > 0 {
		if t, ok := node.Attr("title"); ok && strings.TrimSpace(t) != "" {
			return strings.TrimSpace(t)
		}
		if t := strings.TrimSpace(node.Text()); t != "" {
			return t
		}
	}

	selectors := []string{".main-header .title", "#titlebar .heading", ".heading .title", "h1"}
	for _, css := range selectors {
		n := doc.Find(css).First()
		if n.Length() > 0 {
			if t := strings.TrimSpace(n.Text()); t != "" {
				return t
			}
		}
	}

	title := strings.TrimSpace(doc.Find("title").First().Text())
	if title == "" {
		return ""
	}
	parts := strings.Split(title, " - ")
	return strings.TrimSpace(parts[0])
}

func extractDescriptionNode(doc *goquery.Document) *goquery.Selection {
	selectors := []string{"#legendLife + .detail-content", "#legendLife + .content", ".detail-content", ".article-content", "#legendLife"}
	for _, css := range selectors {
		node := doc.Find(css).First()
		if node.Length() == 0 {
			continue
		}
		if strings.TrimSpace(node.Text()) != "" || node.Find("img").Length() > 0 {
			return node
		}
	}
	return nil
}

func absolutizeMarkdownImageURLs(markdown, pageURL string) (string, error) {
	base, err := url.Parse(pageURL)
	if err != nil {
		return "", fmt.Errorf("解析 bug 页面 URL 失败: %w", err)
	}
	re := regexp.MustCompile(`!\[([^\]]*)\]\(([^)]+)\)`)
	autoNameIndex := 0

	result := re.ReplaceAllStringFunc(markdown, func(m string) string {
		parts := re.FindStringSubmatch(m)
		if len(parts) != 3 {
			return m
		}
		altRaw := strings.TrimSpace(parts[1])
		raw := strings.TrimSpace(parts[2])
		if raw == "" {
			return m
		}
		abs, err := absolutizeURL(base, raw)
		if err != nil {
			return m
		}
		alt := altRaw
		if alt == "" {
			autoNameIndex++
			alt = fmt.Sprintf("img#%d", autoNameIndex)
		}
		return fmt.Sprintf("![%s](%s)", alt, abs)
	})

	return result, nil
}

func normalizeMarkdown(markdown string) string {
	// html-to-markdown may escape bracket text like \[基本信息]。这里统一还原为可读格式。
	markdown = strings.ReplaceAll(markdown, `\[`, `[`)
	markdown = strings.ReplaceAll(markdown, `\]`, `]`)
	return markdown
}

func extractAttachmentURLs(doc *goquery.Document, pageURL string) ([]string, error) {
	base, err := url.Parse(pageURL)
	if err != nil {
		return nil, fmt.Errorf("解析 bug 页面 URL 失败: %w", err)
	}

	var urls []string
	seen := map[string]struct{}{}

	doc.Find("div.detail").Each(func(_ int, detail *goquery.Selection) {
		title := strings.TrimSpace(detail.Find(".detail-title").First().Text())
		if !strings.Contains(title, "附件") {
			return
		}
		detail.Find(".files-list a[href]").Each(func(_ int, a *goquery.Selection) {
			href, ok := a.Attr("href")
			if !ok {
				return
			}
			href = strings.TrimSpace(href)
			if href == "" || strings.HasPrefix(strings.ToLower(href), "javascript:") {
				return
			}
			// 只保留真实附件链接，忽略“重命名”等管理链接。
			if strings.Contains(href, "/file-edit-") {
				return
			}
			u, err := absolutizeURL(base, href)
			if err != nil {
				return
			}
			if _, exists := seen[u]; exists {
				return
			}
			seen[u] = struct{}{}
			urls = append(urls, u)
		})
	})
	return urls, nil
}

func appendAttachmentLinks(markdown string, attachmentURLs []string) string {
	if len(attachmentURLs) == 0 {
		return markdown
	}
	var b strings.Builder
	b.WriteString(strings.TrimSpace(markdown))
	b.WriteString("\n\nAttachments:\n")
	for i, u := range attachmentURLs {
		b.WriteString(fmt.Sprintf("- [attachment#%d](%s)\n", i+1, u))
	}
	return strings.TrimRight(b.String(), "\n")
}

func absolutizeURL(base *url.URL, raw string) (string, error) {
	if strings.HasPrefix(raw, "data:") || strings.HasPrefix(raw, "#") {
		return raw, nil
	}
	u, err := url.Parse(raw)
	if err != nil {
		return "", err
	}
	if u.IsAbs() {
		return u.String(), nil
	}
	return base.ResolveReference(u).String(), nil
}

func RenderMarkdown(id uint64, detail *BugDetail) string {
	var b strings.Builder
	b.WriteString(fmt.Sprintf("# Bug #%d %s\n\n", id, detail.Title))
	b.WriteString("## 描述\n\n")
	if strings.TrimSpace(detail.MarkdownDescription) == "" {
		b.WriteString("(无)\n\n")
	} else {
		b.WriteString(detail.MarkdownDescription)
		b.WriteString("\n\n")
	}
	return b.String()
}
