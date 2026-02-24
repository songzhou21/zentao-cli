package api

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

type ZentaoAPI struct {
	siteURL string
	client  *http.Client
}

func New(siteURL string, _ string) *ZentaoAPI {
	return &ZentaoAPI{
		siteURL: strings.TrimRight(siteURL, "/"),
		client:  &http.Client{},
	}
}

func (z *ZentaoAPI) VerifyCookie(cookie string) (string, error) {
	req, err := http.NewRequest(http.MethodGet, z.siteURL, nil)
	if err != nil {
		return "", fmt.Errorf("创建请求失败: %w", err)
	}
	req.Header.Set("Cookie", cookie)

	resp, err := z.client.Do(req)
	if err != nil {
		return "", fmt.Errorf("请求站点首页失败: %w", err)
	}
	defer resp.Body.Close()

	finalURL := resp.Request.URL.String()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return "", fmt.Errorf("cookie 校验失败: HTTP %d", resp.StatusCode)
	}

	if strings.HasPrefix(finalURL, z.siteURL+"/my/") {
		return finalURL, nil
	}
	if strings.Contains(finalURL, "/user-login-") || strings.Contains(finalURL, "/user-login.") {
		return "", fmt.Errorf("cookie 无效或已过期")
	}
	return "", fmt.Errorf("cookie 校验失败: 未命中预期跳转，最终地址: %s", finalURL)
}

func (z *ZentaoAPI) FetchBugHTML(bugID uint64, cookie string) (string, string, error) {
	bugURL := fmt.Sprintf("%s/bug-view-%d.html", z.siteURL, bugID)
	req, err := http.NewRequest(http.MethodGet, bugURL, nil)
	if err != nil {
		return "", "", fmt.Errorf("创建请求失败: %w", err)
	}
	req.Header.Set("Cookie", cookie)

	resp, err := z.client.Do(req)
	if err != nil {
		return "", "", fmt.Errorf("请求 bug 页面失败: %s (%w)", bugURL, err)
	}
	defer resp.Body.Close()

	body, _ := io.ReadAll(resp.Body)
	finalURL := resp.Request.URL.String()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return "", "", fmt.Errorf("获取 bug 详情失败: HTTP %d (%s)", resp.StatusCode, finalURL)
	}
	if strings.Contains(finalURL, "/user-login-") || strings.Contains(finalURL, "/user-login.") {
		return "", "", fmt.Errorf("获取 bug 详情失败: cookie 无效或已过期")
	}
	if strings.TrimSpace(string(body)) == "" {
		return "", "", fmt.Errorf("获取 bug 详情失败: 页面内容为空")
	}

	return finalURL, string(body), nil
}
