package bug

import (
	"fmt"
	"net/url"
	"path"
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
	autoNameCounts := map[string]int{}

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
			key := deriveImageKey(abs)
			autoNameCounts[key]++
			alt = fmt.Sprintf("img-%s-%d", key, autoNameCounts[key])
		}
		return fmt.Sprintf("![%s](%s)", alt, abs)
	})

	return result, nil
}

func deriveImageKey(absURL string) string {
	u, err := url.Parse(absURL)
	if err != nil {
		return "unknown"
	}
	name := path.Base(u.Path)
	if name == "." || name == "/" || name == "" {
		return "unknown"
	}
	stem := strings.TrimSuffix(name, path.Ext(name))
	numbers := regexp.MustCompile(`(\d+)`).FindAllString(stem, -1)
	if len(numbers) > 0 {
		return numbers[len(numbers)-1]
	}
	if stem == "" {
		return "unknown"
	}
	return stem
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
