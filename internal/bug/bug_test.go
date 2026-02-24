package bug

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func readFixture(t *testing.T, name string) string {
	t.Helper()
	path := filepath.Join("testdata", "bug_html", name)
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture %s: %v", name, err)
	}
	return string(b)
}

func TestParseBugDetail_Real48919Fixture(t *testing.T) {
	// 真实 bug 48919 HTML 应稳定解析出标题、关键描述和图片绝对地址。
	html := readFixture(t, "bug_48919_real.html")
	detail, err := ParseBugDetail("http://shendao.sharexm.cn/zentao/bug-view-48919.html", html)
	if err != nil {
		t.Fatalf("ParseBugDetail real fixture error: %v", err)
	}
	if !strings.Contains(detail.Title, "PC登录后") {
		t.Fatalf("real fixture title mismatch: %q", detail.Title)
	}
	if !strings.Contains(detail.MarkdownDescription, "PC已登录进入登录确认页面") {
		t.Fatalf("real fixture markdown missing key sentence: %s", detail.MarkdownDescription)
	}
	if strings.Contains(detail.MarkdownDescription, `\[`) || strings.Contains(detail.MarkdownDescription, `\]`) {
		t.Fatalf("real fixture markdown should not contain escaped brackets: %s", detail.MarkdownDescription)
	}
	if !strings.Contains(detail.MarkdownDescription, "**[基本信息]**") {
		t.Fatalf("real fixture markdown should contain normalized bracket section: %s", detail.MarkdownDescription)
	}
	if !strings.Contains(detail.MarkdownDescription, "http://shendao.sharexm.cn/zentao/file-read-59561.png") {
		t.Fatalf("real fixture markdown missing absolute image url: %s", detail.MarkdownDescription)
	}
	if !strings.Contains(detail.MarkdownDescription, "![img#1](http://shendao.sharexm.cn/zentao/file-read-59561.png)") {
		t.Fatalf("real fixture markdown should auto name image as img#1: %s", detail.MarkdownDescription)
	}
}

func TestParseBugDetail_Real51267MultipleImagesFixture(t *testing.T) {
	// 真实 bug 51267（正文含多张图片）应输出多张绝对地址图片且按顺序命名。
	html := readFixture(t, "bug_51267_real.html")
	detail, err := ParseBugDetail("http://shendao.sharexm.cn/zentao/bug-view-51267.html", html)
	if err != nil {
		t.Fatalf("ParseBugDetail real fixture 51267 error: %v", err)
	}
	if !strings.Contains(detail.Title, "我的->创作中心") {
		t.Fatalf("real fixture 51267 title mismatch: %q", detail.Title)
	}
	if !strings.Contains(detail.MarkdownDescription, "在我的页面进入创作中心") {
		t.Fatalf("real fixture 51267 markdown missing key sentence: %s", detail.MarkdownDescription)
	}
	if !strings.Contains(detail.MarkdownDescription, "![img#1](http://shendao.sharexm.cn/zentao/file-read-62828.jpeg)") {
		t.Fatalf("real fixture 51267 markdown missing first image: %s", detail.MarkdownDescription)
	}
	if !strings.Contains(detail.MarkdownDescription, "![img#2](http://shendao.sharexm.cn/zentao/file-read-62827.png)") {
		t.Fatalf("real fixture 51267 markdown missing second image: %s", detail.MarkdownDescription)
	}
	if strings.Contains(detail.MarkdownDescription, "Attachments:") {
		t.Fatalf("real fixture 51267 should not append attachment section: %s", detail.MarkdownDescription)
	}
	if strings.Contains(detail.MarkdownDescription, `\[`) || strings.Contains(detail.MarkdownDescription, `\]`) {
		t.Fatalf("real fixture 51267 markdown should not contain escaped brackets: %s", detail.MarkdownDescription)
	}
}

func TestParseBugDetail_MissingTitle(t *testing.T) {
	// 缺标题页面应返回“未解析到 bug 标题”错误。
	html := readFixture(t, "bug_missing_title.html")
	_, err := ParseBugDetail("http://example.com/zentao/bug-view-1.html", html)
	if err == nil || !strings.Contains(err.Error(), "未解析到 bug 标题") {
		t.Fatalf("expected missing title error, got: %v", err)
	}
}

func TestParseBugDetail_MissingDescription(t *testing.T) {
	// 缺描述页面应返回“未解析到 bug 描述”错误。
	html := readFixture(t, "bug_missing_desc.html")
	_, err := ParseBugDetail("http://example.com/zentao/bug-view-1.html", html)
	if err == nil || !strings.Contains(err.Error(), "未解析到 bug 描述") {
		t.Fatalf("expected missing desc error, got: %v", err)
	}
}

func TestAbsolutizeMarkdownImageURLs(t *testing.T) {
	// 相对链接转绝对；绝对链接/data URL 保持不变；空 alt 自动命名。
	in := strings.Join([]string{
		"![ ](/a/1.png)",
		"![x](images/2.jpg)",
		"![y](https://cdn.example.com/3.png)",
		"![d](data:image/png;base64,abc)",
	}, "\n")
	out, err := absolutizeMarkdownImageURLs(in, "http://example.com/zentao/bug-view-1.html")
	if err != nil {
		t.Fatalf("absolutizeMarkdownImageURLs error: %v", err)
	}
	if !strings.Contains(out, "![img#1](http://example.com/a/1.png)") {
		t.Fatalf("missing converted /a/1.png: %s", out)
	}
	if !strings.Contains(out, "![x](http://example.com/zentao/images/2.jpg)") {
		t.Fatalf("missing converted images/2.jpg: %s", out)
	}
	if !strings.Contains(out, "![y](https://cdn.example.com/3.png)") {
		t.Fatalf("unexpected absolute URL rewrite: %s", out)
	}
	if !strings.Contains(out, "![d](data:image/png;base64,abc)") {
		t.Fatalf("unexpected data url rewrite: %s", out)
	}
}

func TestNormalizeMarkdown(t *testing.T) {
	// 转义的方括号应还原为普通方括号，其他文本保持不变。
	in := `**\[基本信息\]**`
	out := normalizeMarkdown(in)
	if out != `**[基本信息]**` {
		t.Fatalf("normalizeMarkdown mismatch: got %q", out)
	}
}

func TestAppendAttachmentLinks(t *testing.T) {
	// 有附件时应追加“附件”段落和编号链接。
	out := appendAttachmentLinks("正文", []string{"http://a", "http://b"})
	if !strings.Contains(out, "正文") || !strings.Contains(out, "Attachments:") {
		t.Fatalf("appendAttachmentLinks should keep body and append section: %s", out)
	}
	if !strings.Contains(out, "- [attachment#1](http://a)") || !strings.Contains(out, "- [attachment#2](http://b)") {
		t.Fatalf("appendAttachmentLinks numbered links mismatch: %s", out)
	}

	// 无附件时不应改动正文。
	unchanged := appendAttachmentLinks("正文", nil)
	if unchanged != "正文" {
		t.Fatalf("appendAttachmentLinks should keep markdown unchanged when empty: %s", unchanged)
	}
}

func TestRenderMarkdown(t *testing.T) {
	// 正常描述应输出标题与正文 section。
	md := RenderMarkdown(9, &BugDetail{Title: "标题", MarkdownDescription: "正文"})
	if !strings.Contains(md, "# Bug #9 标题") || !strings.Contains(md, "## 描述") || !strings.Contains(md, "正文") {
		t.Fatalf("unexpected markdown: %s", md)
	}

	// 空描述应输出“(无)”占位。
	empty := RenderMarkdown(10, &BugDetail{Title: "空描述", MarkdownDescription: "  "})
	if !strings.Contains(empty, "(无)") {
		t.Fatalf("empty description should render (无): %s", empty)
	}
}
