// Go conformance runner (github.com/stellar/go-stellar-sdk/xdr).
//
// The Go SDK keeps an ScMap as an ordered slice of entries and has no
// generic conversion to a native Go map, so the native-conversion check
// does not apply. Every other check runs.
//
//	go run . [-corpus ../../corpus] [-out ../../results]
package main

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"runtime/debug"

	"github.com/stellar/go-stellar-sdk/xdr"
)

const sdkModule = "github.com/stellar/go-stellar-sdk"

type typedValue struct {
	Type    string       `json:"type"`
	Value   *string      `json:"value,omitempty"`
	Entries []typedEntry `json:"entries,omitempty"`
}

type typedEntry struct {
	Key typedValue `json:"key"`
	Val typedValue `json:"val"`
}

type manifestEntry struct {
	ID       string `json:"id"`
	File     string `json:"file"`
	Expected struct {
		Decoded json.RawMessage `json:"decoded"`
	} `json:"expected"`
}

type result struct {
	ID        string `json:"id"`
	Decode    string `json:"decode"`
	Roundtrip string `json:"roundtrip"`
	Structure string `json:"structure"`
	XdrJSON   string `json:"xdr_json"`
	Native    string `json:"native"`
	Detail    string `json:"detail"`
}

func scalar(t, v string) typedValue { return typedValue{Type: t, Value: &v} }

// typed returns the language-neutral form of a decoded value (see corpus/schema).
func typed(v xdr.ScVal) (typedValue, error) {
	switch v.Type {
	case xdr.ScValTypeScvBool:
		return scalar("bool", fmt.Sprintf("%t", *v.B)), nil
	case xdr.ScValTypeScvU32:
		return scalar("u32", fmt.Sprintf("%d", *v.U32)), nil
	case xdr.ScValTypeScvI32:
		return scalar("i32", fmt.Sprintf("%d", *v.I32)), nil
	case xdr.ScValTypeScvU64:
		return scalar("u64", fmt.Sprintf("%d", *v.U64)), nil
	case xdr.ScValTypeScvI64:
		return scalar("i64", fmt.Sprintf("%d", *v.I64)), nil
	case xdr.ScValTypeScvSymbol:
		return scalar("symbol", string(*v.Sym)), nil
	case xdr.ScValTypeScvString:
		return scalar("string", string(*v.Str)), nil
	case xdr.ScValTypeScvMap:
		out := typedValue{Type: "map", Entries: []typedEntry{}}
		if v.Map == nil || *v.Map == nil {
			return out, nil
		}
		for _, e := range **v.Map {
			k, err := typed(e.Key)
			if err != nil {
				return out, err
			}
			val, err := typed(e.Val)
			if err != nil {
				return out, err
			}
			out.Entries = append(out.Entries, typedEntry{Key: k, Val: val})
		}
		return out, nil
	}
	return typedValue{}, fmt.Errorf("no language-neutral form for %s", v.Type)
}

func check(corpus string, e manifestEntry) result {
	r := result{ID: e.ID, Decode: "fail", Roundtrip: "fail", Structure: "fail", XdrJSON: "n/a", Native: "n/a"}

	raw, err := os.ReadFile(filepath.Join(corpus, e.File))
	if err != nil {
		r.Detail = "read: " + err.Error()
		return r
	}
	var v xdr.ScVal
	if err := xdr.SafeUnmarshal(raw, &v); err != nil {
		r.Detail = "decode: " + err.Error()
		return r
	}
	r.Decode = "pass"

	if again, err := v.MarshalBinary(); err == nil && bytes.Equal(again, raw) {
		r.Roundtrip = "pass"
	}

	got, err := typed(v)
	if err != nil {
		r.Detail = "structure: " + err.Error()
		return r
	}
	var want, have any
	gotJSON, _ := json.Marshal(got)
	_ = json.Unmarshal(gotJSON, &have)
	_ = json.Unmarshal(e.Expected.Decoded, &want)
	if reflect.DeepEqual(have, want) {
		r.Structure = "pass"
	} else {
		r.Detail = "structure: got " + string(gotJSON)
	}
	return r
}

func sdkVersion() string {
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, d := range info.Deps {
			if d.Path == sdkModule {
				return d.Version
			}
		}
	}
	return "unknown"
}

func main() {
	corpus := flag.String("corpus", "../../corpus", "corpus directory")
	out := flag.String("out", "../../results", "results directory")
	flag.Parse()

	data, err := os.ReadFile(filepath.Join(*corpus, "manifest.json"))
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	var m struct {
		Entries []manifestEntry `json:"entries"`
	}
	if err := json.Unmarshal(data, &m); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	version := sdkVersion()
	results := make([]result, 0, len(m.Entries))
	fmt.Printf("go %s %s\n", sdkModule, version)
	for _, e := range m.Entries {
		r := check(*corpus, e)
		results = append(results, r)
		fmt.Printf("  %-28s decode=%s roundtrip=%s structure=%s xdr_json=%s native=%s\n",
			r.ID, r.Decode, r.Roundtrip, r.Structure, r.XdrJSON, r.Native)
	}

	report := map[string]any{
		"runner":          "go",
		"sdk":             sdkModule,
		"sdk_version":     version,
		"native_function": nil,
		"runtime":         runtime.Version(),
		"results":         results,
	}
	text, _ := json.MarshalIndent(report, "", "  ")
	if err := os.MkdirAll(*out, 0o755); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	path := filepath.Join(*out, "go-"+version+".json")
	if err := os.WriteFile(path, append(text, '\n'), 0o644); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := runAPI(*corpus, *out, version); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
