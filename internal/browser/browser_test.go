package browser

import (
	"testing"
	"time"
)

func unixToChromeExpires(unix int64) int64 {
	return (unix + 11_644_473_600) * 1_000_000
}

func TestPKCS7Unpad(t *testing.T) {
	// 合法 PKCS7 padding 应被正确去除。
	valid := append([]byte("DATA"), []byte{4, 4, 4, 4}...)
	out, err := pkcs7Unpad(valid)
	if err != nil {
		t.Fatalf("valid unpad should not error: %v", err)
	}
	if string(out) != "DATA" {
		t.Fatalf("unexpected unpad output: %q", string(out))
	}

	// 非法 padding 应返回错误。
	_, err = pkcs7Unpad([]byte{1, 2, 3, 0})
	if err == nil {
		t.Fatalf("invalid padding should error")
	}
}

func TestHostMatches(t *testing.T) {
	cases := []struct {
		note   string
		target string
		cookie string
		want   bool
	}{
		{"测子域命中；预期 true", "a.example.com", ".example.com", true},
		{"测根域命中；预期 true", "example.com", ".example.com", true},
		{"测完全相等域名；预期 true", "example.com", "example.com", true},
		{"测无关域名；预期 false", "bad.com", ".example.com", false},
	}
	for _, tc := range cases {
		// 参考 table note。
		if got := hostMatches(tc.target, tc.cookie); got != tc.want {
			t.Fatalf("hostMatches(%q,%q)=%v want %v", tc.target, tc.cookie, got, tc.want)
		}
	}
}

func TestNormalizePath(t *testing.T) {
	// 空路径归一化为根路径。
	if normalizePath("") != "/" {
		t.Fatalf("empty path should normalize to /")
	}
	// 缺少末尾 / 时自动补全。
	if normalizePath("/zentao") != "/zentao/" {
		t.Fatalf("missing trailing slash should be appended")
	}
	// 已有末尾 / 不应被改变。
	if normalizePath("/zentao/") != "/zentao/" {
		t.Fatalf("existing trailing slash should remain")
	}
}

func TestChromeExpiresUTCToUnix(t *testing.T) {
	// 0 代表 session，转换后仍为 0。
	if chromeExpiresUTCToUnix(0) != 0 {
		t.Fatalf("zero expires should map to zero unix")
	}
	// Chrome epoch 精确转换为 Unix epoch。
	if got := chromeExpiresUTCToUnix(11_644_473_600 * 1_000_000); got != 0 {
		t.Fatalf("chrome epoch mismatch: got %d", got)
	}
}

func TestChooseBestByPath(t *testing.T) {
	// 同名 cookie 选择“未过期 + path 最长”的项。
	future := unixToChromeExpires(time.Now().Add(24 * time.Hour).Unix())
	past := unixToChromeExpires(time.Now().Add(-24 * time.Hour).Unix())

	items := []BrowserCookieItem{
		{Name: "zp", Value: "expired", Path: "/zentao/deep/", ExpiresUTC: past},
		{Name: "zp", Value: "root", Path: "/", ExpiresUTC: future},
		{Name: "zp", Value: "deep", Path: "/zentao/", ExpiresUTC: future},
		{Name: "za", Value: "other", Path: "/zentao/", ExpiresUTC: future},
	}
	best := chooseBestByPath(items, "zp")
	if best == nil {
		t.Fatalf("expected best item")
	}
	if best.Value != "deep" {
		t.Fatalf("expected deepest non-expired path, got %q", best.Value)
	}
}
