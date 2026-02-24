package cli

import (
	"bufio"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"zentao-cli/internal/api"
	"zentao-cli/internal/browser"
	"zentao-cli/internal/bug"
	"zentao-cli/internal/config"
)

func Run(args []string) error {
	if len(args) == 0 {
		return usageError()
	}

	switch args[0] {
	case "cookie":
		return runCookie(args[1:])
	case "chrome":
		return runChrome(args[1:])
	case "bug":
		return runBug(args[1:])
	case "-h", "--help", "help":
		printHelp()
		return nil
	default:
		return usageError()
	}
}

func runCookie(args []string) error {
	fs := flag.NewFlagSet("cookie", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	urlFlag := fs.String("url", "", "禅道地址")
	profileFlag := fs.String("profile", "", "Chrome profile 目录")
	verifyFlag := fs.Bool("verify", false, "校验 cookie 是否有效")
	apiVersionFlag := fs.String("api-version", "", "API 版本")
	configFlag := fs.String("config", "", "配置文件路径")
	if err := fs.Parse(args); err != nil {
		return err
	}

	cfgPath, err := resolveConfigPath(*configFlag)
	if err != nil {
		return err
	}
	cfg, err := config.LoadConfigOptional(cfgPath)
	if err != nil {
		return err
	}

	siteURL, err := resolveRequired(*urlFlag, getConfigString(cfg, func(c *config.Config) string { return c.URL }), "url")
	if err != nil {
		return err
	}
	apiVersion := strings.TrimSpace(*apiVersionFlag)
	if apiVersion == "" {
		apiVersion = getConfigString(cfg, func(c *config.Config) string { return c.APIVersion })
	}
	if apiVersion == "" {
		apiVersion = "v1"
	}

	profile := strings.TrimSpace(*profileFlag)
	if profile == "" && cfg != nil && cfg.ChromeProfile != nil {
		profile = *cfg.ChromeProfile
	}

	cookie, err := browser.LoadZentaoCookieFromChromeMacOS(siteURL, profile)
	if err != nil {
		return err
	}
	expiry := "unknown"
	if len(cookie.Items) > 0 {
		expiry = formatCookieExpiry(cookie.Items[0].ExpiresUTC)
	}
	fmt.Printf("\x1b[1;33m过期时间: %s\x1b[0m\n", expiry)
	fmt.Println("浏览器 cookie 明细:")
	for _, item := range cookie.Items {
		fmt.Printf("- %s: value=%s, domain=%s, path=%s, secure=%t, httpOnly=%t\n",
			item.Name, item.Value, item.Domain, item.Path, item.Secure, item.HTTPOnly)
	}

	if *verifyFlag {
		client := api.New(siteURL, apiVersion)
		finalURL, err := client.VerifyCookie(cookie.CookieHeader)
		if err != nil {
			fmt.Printf("\x1b[1;31mcookie 校验失败: %s\x1b[0m\n", err)
			return err
		}
		fmt.Printf("\x1b[1;32mcookie 校验成功，最终跳转: %s\x1b[0m\n", finalURL)
	}

	return nil
}

func runChrome(args []string) error {
	if len(args) == 0 || args[0] != "profile" {
		return fmt.Errorf("用法: zentao chrome profile [--config PATH]")
	}
	fs := flag.NewFlagSet("chrome profile", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	configFlag := fs.String("config", "", "配置文件路径")
	if err := fs.Parse(args[1:]); err != nil {
		return err
	}

	cfgPath, err := resolveConfigPath(*configFlag)
	if err != nil {
		return err
	}
	cfg, err := config.LoadOrDefault(cfgPath)
	if err != nil {
		return err
	}

	profiles, err := browser.ListChromeProfilesMacOS()
	if err != nil {
		return err
	}
	if len(profiles) == 0 {
		return fmt.Errorf("未找到可用的 Chrome profile")
	}

	if cfg.ChromeProfile != nil {
		fmt.Printf("当前已选择: %s\n", *cfg.ChromeProfile)
	} else {
		fmt.Println("当前已选择: (未设置)")
	}
	fmt.Println("可用 Chrome profiles:")
	for i, profile := range profiles {
		marker := ""
		if cfg.ChromeProfile != nil && *cfg.ChromeProfile == profile {
			marker = " \x1b[1;32m[当前]\x1b[0m"
		}
		fmt.Printf("%d. %s%s\n", i+1, profile, marker)
	}
	fmt.Print("请输入编号（输入 q 退出）: ")

	reader := bufio.NewReader(os.Stdin)
	input, err := reader.ReadString('\n')
	if err != nil {
		return fmt.Errorf("读取输入失败: %w", err)
	}
	input = strings.TrimSpace(input)
	if strings.EqualFold(input, "q") {
		fmt.Println("已取消选择")
		return nil
	}
	index, err := strconv.Atoi(input)
	if err != nil {
		return fmt.Errorf("输入无效，请输入数字编号")
	}
	if index <= 0 || index > len(profiles) {
		return fmt.Errorf("编号超出范围，请输入 1-%d", len(profiles))
	}

	selected := profiles[index-1]
	cfg.ChromeProfile = &selected
	if err := config.SaveConfig(cfgPath, cfg); err != nil {
		return err
	}
	fmt.Printf("已保存 chrome_profile: %s\n", selected)
	fmt.Printf("配置文件: %s\n", cfgPath)
	return nil
}

func runBug(args []string) error {
	if len(args) == 0 || args[0] != "show" {
		return fmt.Errorf("用法: zentao bug show <id> [--url URL] [--profile PATH] [--config PATH] [--out FILE]")
	}
	if len(args) < 2 {
		return fmt.Errorf("缺少 Bug ID")
	}

	id, err := strconv.ParseUint(args[1], 10, 64)
	if err != nil {
		return fmt.Errorf("Bug ID 无效: %w", err)
	}

	fs := flag.NewFlagSet("bug show", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	urlFlag := fs.String("url", "", "禅道地址")
	profileFlag := fs.String("profile", "", "Chrome profile 目录")
	configFlag := fs.String("config", "", "配置文件路径")
	outFlag := fs.String("out", "", "输出 Markdown 文件")
	if err := fs.Parse(args[2:]); err != nil {
		return err
	}

	cfgPath, err := resolveConfigPath(*configFlag)
	if err != nil {
		return err
	}
	cfg, err := config.LoadConfigOptional(cfgPath)
	if err != nil {
		return err
	}

	siteURL, err := resolveRequired(*urlFlag, getConfigString(cfg, func(c *config.Config) string { return c.URL }), "url")
	if err != nil {
		return err
	}
	profile := strings.TrimSpace(*profileFlag)
	if profile == "" && cfg != nil && cfg.ChromeProfile != nil {
		profile = *cfg.ChromeProfile
	}

	apiClient := api.New(siteURL, "v1")
	cookie, err := browser.LoadZentaoCookieFromChromeMacOS(siteURL, profile)
	if err != nil {
		return err
	}
	finalURL, html, err := apiClient.FetchBugHTML(id, cookie.CookieHeader)
	if err != nil {
		return err
	}
	bugDetail, err := bug.ParseBugDetail(finalURL, html)
	if err != nil {
		return err
	}
	markdown := bug.RenderMarkdown(id, bugDetail)

	outPath := strings.TrimSpace(*outFlag)
	if outPath != "" {
		if err := os.MkdirAll(filepath.Dir(outPath), 0o755); err != nil {
			return fmt.Errorf("创建输出目录失败: %w", err)
		}
		if err := os.WriteFile(outPath, []byte(markdown), 0o644); err != nil {
			return fmt.Errorf("写入 Markdown 失败: %w", err)
		}
		fmt.Printf("Markdown 已写入 %s\n", outPath)
		return nil
	}

	fmt.Print(markdown)
	return nil
}

func resolveRequired(fromCLI, fromCfg, field string) (string, error) {
	fromCLI = strings.TrimSpace(fromCLI)
	if fromCLI != "" {
		return fromCLI, nil
	}
	fromCfg = strings.TrimSpace(fromCfg)
	if fromCfg != "" {
		return fromCfg, nil
	}
	return "", fmt.Errorf("缺少 %s，请通过命令行参数或配置文件提供", field)
}

func getConfigString(cfg *config.Config, getter func(*config.Config) string) string {
	if cfg == nil {
		return ""
	}
	return getter(cfg)
}

func resolveConfigPath(cliPath string) (string, error) {
	if strings.TrimSpace(cliPath) != "" {
		return cliPath, nil
	}
	return config.DefaultConfigPath()
}

func formatCookieExpiry(expiresUTC int64) string {
	unix := chromeExpiresUTCToUnix(expiresUTC)
	if unix <= 0 {
		return "session"
	}
	return time.Unix(unix, 0).UTC().Format("2006-01-02 15:04:05 UTC")
}

func chromeExpiresUTCToUnix(expiresUTC int64) int64 {
	if expiresUTC <= 0 {
		return 0
	}
	return (expiresUTC / 1_000_000) - 11_644_473_600
}

func usageError() error {
	printHelp()
	return fmt.Errorf("无效命令")
}

func printHelp() {
	fmt.Println("zentao <command>")
	fmt.Println("commands:")
	fmt.Println("  cookie --url <url> [--profile <path>] [--verify] [--api-version <v>] [--config <path>]")
	fmt.Println("  chrome profile [--config <path>]")
	fmt.Println("  bug show <id> --url <url> [--profile <path>] [--config <path>] [--out <file>]")
}
