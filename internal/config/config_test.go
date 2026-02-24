package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func strPtr(s string) *string { return &s }

func TestDefaultConfigPath(t *testing.T) {
	// 默认配置路径应落在 ~/.zentao/config.json。
	p, err := DefaultConfigPath()
	if err != nil {
		t.Fatalf("DefaultConfigPath error: %v", err)
	}
	if !strings.HasSuffix(filepath.ToSlash(p), "/.zentao/config.json") {
		t.Fatalf("unexpected default config path: %s", p)
	}
}

func TestSaveAndLoadConfig(t *testing.T) {
	// 配置写入后应可无损读取。
	dir := t.TempDir()
	path := filepath.Join(dir, "nested", "config.json")
	cfg := &Config{URL: "http://example.com/zentao", ChromeProfile: strPtr("/tmp/profile")}
	if err := SaveConfig(path, cfg); err != nil {
		t.Fatalf("SaveConfig error: %v", err)
	}
	loaded, err := LoadConfig(path)
	if err != nil {
		t.Fatalf("LoadConfig error: %v", err)
	}
	if loaded.URL != cfg.URL {
		t.Fatalf("URL mismatch: got %q want %q", loaded.URL, cfg.URL)
	}
	if loaded.ChromeProfile == nil || *loaded.ChromeProfile != "/tmp/profile" {
		t.Fatalf("ChromeProfile mismatch: %#v", loaded.ChromeProfile)
	}
}

func TestLoadConfigOptionalAndDefault(t *testing.T) {
	dir := t.TempDir()
	missing := filepath.Join(dir, "missing.json")
	// 文件不存在时 LoadConfigOptional 返回 (nil, nil)。
	cfg, err := LoadConfigOptional(missing)
	if err != nil {
		t.Fatalf("LoadConfigOptional missing error: %v", err)
	}
	if cfg != nil {
		t.Fatalf("LoadConfigOptional missing should return nil config")
	}

	// 文件不存在时 LoadOrDefault 返回非 nil 默认配置。
	defaultCfg, err := LoadOrDefault(missing)
	if err != nil {
		t.Fatalf("LoadOrDefault error: %v", err)
	}
	if defaultCfg == nil {
		t.Fatalf("LoadOrDefault should return non-nil config")
	}
}

func TestLoadConfigOptionalInvalidJSON(t *testing.T) {
	// 非法 JSON 应返回“配置文件存在但无法解析”错误。
	dir := t.TempDir()
	path := filepath.Join(dir, "bad.json")
	if err := os.WriteFile(path, []byte("{"), 0o644); err != nil {
		t.Fatalf("write bad json: %v", err)
	}
	_, err := LoadConfigOptional(path)
	if err == nil || !strings.Contains(err.Error(), "配置文件存在但无法解析") {
		t.Fatalf("expected parse error, got: %v", err)
	}
}
