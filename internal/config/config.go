package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

type Config struct {
	URL           string  `json:"url"`
	Username      *string `json:"username,omitempty"`
	Password      *string `json:"password,omitempty"`
	Code          *string `json:"code,omitempty"`
	Token         *string `json:"token,omitempty"`
	APIVersion    string  `json:"api_version"`
	ChromeProfile *string `json:"chrome_profile,omitempty"`
}

func DefaultConfigPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("无法定位用户主目录: %w", err)
	}
	return filepath.Join(home, ".zentao", "config.json"), nil
}

func LoadConfig(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("读取配置失败: %s (%w)", path, err)
	}
	var cfg Config
	if err := json.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("解析配置失败: %s (%w)", path, err)
	}
	if cfg.APIVersion == "" {
		cfg.APIVersion = "v1"
	}
	return &cfg, nil
}

func SaveConfig(path string, cfg *Config) error {
	if cfg.APIVersion == "" {
		cfg.APIVersion = "v1"
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("创建配置目录失败: %w", err)
	}
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return fmt.Errorf("序列化配置失败: %w", err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		return fmt.Errorf("写入配置失败: %s (%w)", path, err)
	}
	return nil
}

func LoadConfigOptional(path string) (*Config, error) {
	_, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("检查配置文件失败: %s (%w)", path, err)
	}
	cfg, err := LoadConfig(path)
	if err != nil {
		return nil, fmt.Errorf("配置文件存在但无法解析，请修复后重试: %s (%w)", path, err)
	}
	return cfg, nil
}

func LoadOrDefault(path string) (*Config, error) {
	cfg, err := LoadConfigOptional(path)
	if err != nil {
		return nil, err
	}
	if cfg != nil {
		return cfg, nil
	}
	return &Config{APIVersion: "v1"}, nil
}
