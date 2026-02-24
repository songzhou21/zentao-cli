package cli

import (
	"strings"
	"testing"
)

func TestResolveRequired(t *testing.T) {
	tests := []struct {
		name    string
		note    string
		fromCLI string
		fromCfg string
		want    string
		wantErr string
	}{
		{name: "prefer cli", note: "测命令行参数优先；预期返回 fromCLI", fromCLI: "http://cli", fromCfg: "http://cfg", want: "http://cli"},
		{name: "fallback cfg", note: "测命令行为空时回退配置；预期返回 fromCfg", fromCfg: "http://cfg", want: "http://cfg"},
		{name: "missing", note: "测两者都缺失；预期返回缺参错误", wantErr: "缺少 url"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// 参考 table note。
			got, err := resolveRequired(tt.fromCLI, tt.fromCfg, "url")
			if tt.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
					t.Fatalf("expected error containing %q, got: %v", tt.wantErr, err)
				}
				return
			}
			if err != nil {
				t.Fatalf("resolveRequired error: %v", err)
			}
			if got != tt.want {
				t.Fatalf("resolveRequired got %q want %q", got, tt.want)
			}
		})
	}
}

func TestFormatCookieExpiry(t *testing.T) {
	// expiresUTC=0 表示 session。
	if got := formatCookieExpiry(0); got != "session" {
		t.Fatalf("expected session, got %q", got)
	}

	// 有效 expiresUTC 应格式化为 UTC 时间字符串。
	expiresUTC := (int64(1704067200) + 11_644_473_600) * 1_000_000 // 2024-01-01 00:00:00 UTC
	got := formatCookieExpiry(expiresUTC)
	if got != "2024-01-01 00:00:00 UTC" {
		t.Fatalf("unexpected formatted expiry: %q", got)
	}
}

func TestChromeExpiresUTCToUnix(t *testing.T) {
	// expiresUTC=0 转换后为 0。
	if chromeExpiresUTCToUnix(0) != 0 {
		t.Fatalf("expected 0")
	}
	// 已知时间戳应可精确往返转换。
	expiresUTC := (int64(1704067200) + 11_644_473_600) * 1_000_000
	if got := chromeExpiresUTCToUnix(expiresUTC); got != 1704067200 {
		t.Fatalf("unexpected unix ts: %d", got)
	}
}
