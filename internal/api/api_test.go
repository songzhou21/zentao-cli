package api

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestVerifyCookie(t *testing.T) {
	tests := []struct {
		name       string
		note       string
		status     int
		redirectTo string
		wantErr    string
		wantURL    string
	}{
		{name: "success my redirect", note: "测有效 cookie 跳转到 /my/；预期返回最终地址", status: http.StatusFound, redirectTo: "/my/", wantURL: "/my/"},
		{name: "login redirect", note: "测无效 cookie 跳到登录页；预期返回无效错误", status: http.StatusFound, redirectTo: "/user-login-L3plbnRhby8=.html", wantErr: "cookie 无效或已过期"},
		{name: "unexpected redirect", note: "测跳转到非预期业务页；预期返回未命中错误", status: http.StatusFound, redirectTo: "/project-index.html", wantErr: "未命中预期跳转"},
		{name: "http non 2xx", note: "测首页返回非 2xx；预期返回 HTTP 错误", status: http.StatusInternalServerError, wantErr: "cookie 校验失败: HTTP 500"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// 参考 table note。
			var gotCookie string
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gotCookie = r.Header.Get("Cookie")
				if r.URL.Path == "/" {
					if tt.redirectTo != "" {
						http.Redirect(w, r, tt.redirectTo, tt.status)
						return
					}
					w.WriteHeader(tt.status)
					return
				}
				w.WriteHeader(http.StatusOK)
			}))
			defer srv.Close()

			api := New(srv.URL, "v1")
			finalURL, err := api.VerifyCookie("zp=test")
			if gotCookie != "zp=test" {
				t.Fatalf("cookie header not sent, got: %q", gotCookie)
			}

			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("expected error containing %q, got: %v", tt.wantErr, err)
				}
				return
			}
			if err != nil {
				t.Fatalf("VerifyCookie error: %v", err)
			}
			if !strings.HasSuffix(finalURL, tt.wantURL) {
				t.Fatalf("unexpected finalURL: %s", finalURL)
			}
		})
	}
}

func TestFetchBugHTML(t *testing.T) {
	tests := []struct {
		name         string
		note         string
		status       int
		redirectTo   string
		body         string
		wantErr      string
		wantContains string
	}{
		{name: "success", note: "测正常返回 bug 页面；预期拿到 finalURL 和 body", status: http.StatusOK, body: "<html><body>ok</body></html>", wantContains: "ok"},
		{name: "empty body", note: "测页面为空白；预期返回页面内容为空错误", status: http.StatusOK, body: "   ", wantErr: "页面内容为空"},
		{name: "login redirect", note: "测被重定向到登录页；预期返回 cookie 无效错误", status: http.StatusFound, redirectTo: "/user-login-abc.html", wantErr: "cookie 无效或已过期"},
		{name: "http non 2xx", note: "测 HTTP 非 2xx；预期返回状态码错误", status: http.StatusForbidden, body: "forbidden", wantErr: "HTTP 403"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// 参考 table note。
			var gotCookie string
			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gotCookie = r.Header.Get("Cookie")
				if strings.HasPrefix(r.URL.Path, "/bug-view-") {
					if tt.redirectTo != "" {
						http.Redirect(w, r, tt.redirectTo, tt.status)
						return
					}
					w.WriteHeader(tt.status)
					_, _ = w.Write([]byte(tt.body))
					return
				}
				w.WriteHeader(http.StatusOK)
			}))
			defer srv.Close()

			api := New(srv.URL, "v1")
			finalURL, body, err := api.FetchBugHTML(51214, "zp=test")
			if gotCookie != "zp=test" {
				t.Fatalf("cookie header not sent, got: %q", gotCookie)
			}

			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("expected error containing %q, got: %v", tt.wantErr, err)
				}
				return
			}
			if err != nil {
				t.Fatalf("FetchBugHTML error: %v", err)
			}
			if !strings.Contains(finalURL, "/bug-view-51214.html") {
				t.Fatalf("unexpected finalURL: %s", finalURL)
			}
			if !strings.Contains(body, tt.wantContains) {
				t.Fatalf("unexpected body: %s", body)
			}
		})
	}
}
