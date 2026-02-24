package browser

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"database/sql"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"time"
	"unicode/utf8"

	"crypto/sha1"
	_ "github.com/mattn/go-sqlite3"
	"golang.org/x/crypto/pbkdf2"
)

type BrowserCookieItem struct {
	Name       string
	Value      string
	Domain     string
	Path       string
	Secure     bool
	HTTPOnly   bool
	ExpiresUTC int64
}

type BrowserCookieResult struct {
	CookieHeader string
	Items        []BrowserCookieItem
}

func ListChromeProfilesMacOS() ([]string, error) {
	if runtime.GOOS != "darwin" {
		return nil, fmt.Errorf("当前仅支持 macOS")
	}
	root, err := chromeProfilesRoot()
	if err != nil {
		return nil, err
	}
	return collectChromeProfiles(root)
}

func LoadZentaoCookieFromChromeMacOS(siteURL string, profileOverride string) (*BrowserCookieResult, error) {
	if runtime.GOOS != "darwin" {
		return nil, fmt.Errorf("当前仅支持 macOS")
	}

	u, err := url.Parse(siteURL)
	if err != nil {
		return nil, fmt.Errorf("解析 URL 失败: %w", err)
	}
	host := u.Hostname()
	if host == "" {
		return nil, fmt.Errorf("URL 缺少 host")
	}
	sitePath := normalizePath(u.Path)

	profileDir := profileOverride
	if profileDir == "" {
		profileDir, err = findLatestChromeProfile()
		if err != nil {
			return nil, err
		}
	}

	dbPath := filepath.Join(profileDir, "Cookies")
	if _, err := os.Stat(dbPath); err != nil {
		return nil, fmt.Errorf("未找到 Chrome Cookies 数据库: %s", dbPath)
	}

	temp, err := os.CreateTemp("", "zentao-cookies-*.sqlite")
	if err != nil {
		return nil, fmt.Errorf("创建临时数据库失败: %w", err)
	}
	tempDB := temp.Name()
	_ = temp.Close()
	defer os.Remove(tempDB)
	defer os.Remove(tempDB + "-wal")
	defer os.Remove(tempDB + "-shm")

	if err := copyFile(dbPath, tempDB); err != nil {
		return nil, fmt.Errorf("复制 Cookies 数据库失败: %s (%w)", dbPath, err)
	}
	_ = copyFile(dbPath+"-wal", tempDB+"-wal")
	_ = copyFile(dbPath+"-shm", tempDB+"-shm")

	db, err := sql.Open("sqlite3", tempDB)
	if err != nil {
		return nil, fmt.Errorf("打开 Cookies 数据库失败: %s (%w)", tempDB, err)
	}
	defer db.Close()

	key, err := chromeSafeStorageKey()
	if err != nil {
		return nil, err
	}

	rows, err := db.Query(
		`SELECT name, value, encrypted_value, path, host_key, is_secure, is_httponly, expires_utc
		 FROM cookies
		 WHERE host_key LIKE ?
		   AND name IN ('za', 'zentaosid', 'zp')`,
		"%"+host,
	)
	if err != nil {
		return nil, fmt.Errorf("查询 Cookies 失败: %w", err)
	}
	defer rows.Close()

	var candidates []BrowserCookieItem
	for rows.Next() {
		var name, value, path, rowHost string
		var encrypted []byte
		var secure, httpOnly int64
		var expiresUTC int64
		if err := rows.Scan(&name, &value, &encrypted, &path, &rowHost, &secure, &httpOnly, &expiresUTC); err != nil {
			return nil, fmt.Errorf("读取 Cookies 失败: %w", err)
		}
		if !hostMatches(host, rowHost) {
			continue
		}
		if !strings.HasPrefix(sitePath, normalizePath(path)) {
			continue
		}

		cookieValue := value
		if cookieValue == "" {
			cookieValue, err = decryptChromeCookieValue(encrypted, key)
			if err != nil {
				return nil, err
			}
		}
		candidates = append(candidates, BrowserCookieItem{
			Name:       name,
			Value:      cookieValue,
			Domain:     rowHost,
			Path:       path,
			Secure:     secure != 0,
			HTTPOnly:   httpOnly != 0,
			ExpiresUTC: expiresUTC,
		})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("读取 Cookies 结果失败: %w", err)
	}

	bestZA := chooseBestByPath(candidates, "za")
	bestSID := chooseBestByPath(candidates, "zentaosid")
	bestZP := chooseBestByPath(candidates, "zp")
	if bestZP == nil {
		return nil, fmt.Errorf("Chrome 中未找到匹配站点的 zp cookie")
	}

	parts := make([]string, 0, 3)
	items := make([]BrowserCookieItem, 0, 3)
	if bestZA != nil {
		parts = append(parts, fmt.Sprintf("za=%s", bestZA.Value))
		items = append(items, *bestZA)
	}
	if bestSID != nil {
		parts = append(parts, fmt.Sprintf("zentaosid=%s", bestSID.Value))
		items = append(items, *bestSID)
	}
	parts = append(parts, fmt.Sprintf("zp=%s", bestZP.Value))
	items = append(items, *bestZP)

	return &BrowserCookieResult{CookieHeader: strings.Join(parts, "; "), Items: items}, nil
}

func chromeSafeStorageKey() ([]byte, error) {
	services := []string{"Chrome Safe Storage", "Chromium Safe Storage"}
	lastErr := ""
	for _, service := range services {
		out, err := exec.Command("security", "find-generic-password", "-w", "-s", service).CombinedOutput()
		if err != nil {
			lastErr = strings.TrimSpace(string(out))
			continue
		}
		passphrase := strings.TrimSpace(string(out))
		if passphrase == "" {
			continue
		}
		key := pbkdf2.Key([]byte(passphrase), []byte("saltysalt"), 1003, 16, sha1.New)
		return key, nil
	}
	return nil, fmt.Errorf("读取 Chrome Safe Storage 失败（已尝试 Chrome/Chromium）: %s", lastErr)
}

func decryptChromeCookieValue(encrypted []byte, key []byte) (string, error) {
	if len(encrypted) == 0 {
		return "", nil
	}
	payload := encrypted
	if bytes.HasPrefix(encrypted, []byte("v10")) || bytes.HasPrefix(encrypted, []byte("v11")) {
		payload = encrypted[3:]
	}
	if len(payload) == 0 || len(payload)%aes.BlockSize != 0 {
		return "", fmt.Errorf("解密 Chrome cookie 失败")
	}

	block, err := aes.NewCipher(key)
	if err != nil {
		return "", fmt.Errorf("初始化 Chrome cookie 解密器失败: %w", err)
	}
	iv := bytes.Repeat([]byte{' '}, aes.BlockSize)
	plain := make([]byte, len(payload))
	cipher.NewCBCDecrypter(block, iv).CryptBlocks(plain, payload)
	plain, err = pkcs7Unpad(plain)
	if err != nil {
		return "", fmt.Errorf("解密 Chrome cookie 失败")
	}

	if s := string(plain); isValidUTF8(plain) {
		return s, nil
	}
	if len(plain) > 32 && isValidUTF8(plain[32:]) {
		return string(plain[32:]), nil
	}
	return "", fmt.Errorf("Chrome cookie 不是有效 UTF-8")
}

func pkcs7Unpad(data []byte) ([]byte, error) {
	if len(data) == 0 {
		return nil, fmt.Errorf("invalid padding")
	}
	pad := int(data[len(data)-1])
	if pad <= 0 || pad > len(data) {
		return nil, fmt.Errorf("invalid padding")
	}
	for i := len(data) - pad; i < len(data); i++ {
		if int(data[i]) != pad {
			return nil, fmt.Errorf("invalid padding")
		}
	}
	return data[:len(data)-pad], nil
}

func isValidUTF8(b []byte) bool {
	return utf8.Valid(b)
}

func chooseBestByPath(items []BrowserCookieItem, name string) *BrowserCookieItem {
	var best *BrowserCookieItem
	for i := range items {
		it := items[i]
		if it.Name != name || !isCookieNotExpired(it.ExpiresUTC) {
			continue
		}
		if best == nil || len(it.Path) > len(best.Path) {
			v := it
			best = &v
		}
	}
	return best
}

func isCookieNotExpired(expiresUTC int64) bool {
	if expiresUTC <= 0 {
		return true
	}
	return chromeExpiresUTCToUnix(expiresUTC) > time.Now().Unix()
}

func chromeExpiresUTCToUnix(expiresUTC int64) int64 {
	if expiresUTC <= 0 {
		return 0
	}
	return (expiresUTC / 1_000_000) - 11_644_473_600
}

func hostMatches(targetHost, cookieHost string) bool {
	if cookieHost == targetHost {
		return true
	}
	if strings.HasPrefix(cookieHost, ".") {
		stripped := strings.TrimPrefix(cookieHost, ".")
		return targetHost == stripped || strings.HasSuffix(targetHost, "."+stripped)
	}
	return false
}

func normalizePath(path string) string {
	if path == "" {
		return "/"
	}
	if strings.HasSuffix(path, "/") {
		return path
	}
	return path + "/"
}

func findLatestChromeProfile() (string, error) {
	root, err := chromeProfilesRoot()
	if err != nil {
		return "", err
	}
	profiles, err := collectChromeProfiles(root)
	if err != nil {
		return "", err
	}
	if len(profiles) == 0 {
		return "", fmt.Errorf("未找到 Chrome profile（含 Cookies）")
	}

	type pair struct {
		path string
		time time.Time
	}
	withTime := make([]pair, 0, len(profiles))
	for _, p := range profiles {
		fi, err := os.Stat(filepath.Join(p, "Cookies"))
		t := time.Unix(0, 0)
		if err == nil {
			t = fi.ModTime()
		}
		withTime = append(withTime, pair{path: p, time: t})
	}
	sort.Slice(withTime, func(i, j int) bool { return withTime[i].time.Before(withTime[j].time) })
	return withTime[len(withTime)-1].path, nil
}

func chromeProfilesRoot() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("无法定位用户主目录: %w", err)
	}
	return filepath.Join(home, "Library", "Application Support", "Google", "Chrome"), nil
}

func collectChromeProfiles(root string) ([]string, error) {
	entries, err := os.ReadDir(root)
	if err != nil {
		return nil, fmt.Errorf("读取目录失败: %s (%w)", root, err)
	}
	profiles := make([]string, 0)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		name := entry.Name()
		if name != "Default" && !strings.HasPrefix(name, "Profile ") {
			continue
		}
		path := filepath.Join(root, name)
		if _, err := os.Stat(filepath.Join(path, "Cookies")); err == nil {
			profiles = append(profiles, path)
		}
	}
	sort.Strings(profiles)
	for i, p := range profiles {
		if filepath.Base(p) == "Default" {
			profiles = append([]string{p}, append(profiles[:i], profiles[i+1:]...)...)
			break
		}
	}
	return profiles, nil
}

func copyFile(src, dst string) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()
	if _, err := out.ReadFrom(in); err != nil {
		return err
	}
	return out.Sync()
}
