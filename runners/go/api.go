package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"

	"github.com/stellar/go-stellar-sdk/xdr"
)

// API vectors: cases that exercise the SDK's builder API rather than its
// decoder. See corpus/api-vectors.json.

type apiAsset struct {
	Code   string `json:"code"`
	Issuer string `json:"issuer"`
}

type apiVector struct {
	ID    string `json:"id"`
	Kind  string `json:"kind"`
	Input struct {
		AssetA  apiAsset `json:"asset_a"`
		AssetB  apiAsset `json:"asset_b"`
		Fee     int32    `json:"fee"`
		Account string   `json:"account"`
		MuxedID string   `json:"id"`
	} `json:"input"`
	Expected struct {
		Outcome     string `json:"outcome"`
		PoolID      string `json:"pool_id"`
		CanonicalID string `json:"canonical_id"`
		Address     string `json:"address"`
	} `json:"expected"`
}

type apiResult struct {
	ID     string `json:"id"`
	Status string `json:"status"`
	Detail string `json:"detail"`
}

func liquidityPoolAsset(v apiVector) apiResult {
	r := apiResult{ID: v.ID, Status: "ok"}
	a, errA := xdr.NewCreditAsset(v.Input.AssetA.Code, v.Input.AssetA.Issuer)
	b, errB := xdr.NewCreditAsset(v.Input.AssetB.Code, v.Input.AssetB.Issuer)
	if errA != nil || errB != nil {
		return apiResult{ID: v.ID, Status: "defect", Detail: "runner could not build the assets"}
	}
	id, err := xdr.NewPoolId(a, b, xdr.Int32(v.Input.Fee))
	got := hex.EncodeToString(id[:])
	switch {
	case err != nil && v.Expected.Outcome == "reject":
	case err != nil:
		r.Status, r.Detail = "defect", "rejected a pair the network accepts: "+err.Error()
	case v.Expected.Outcome == "reject":
		r.Status, r.Detail = "defect", "accepted an out-of-order pair as pool "+got
	case got != v.Expected.PoolID:
		r.Status, r.Detail = "defect", fmt.Sprintf("pool id %s, expected %s", got, v.Expected.PoolID)
	}
	return r
}

// muxedAccountID: the Go SDK takes a muxed ID as a uint64, so the vectors
// that pass a non-canonical string have no matching API. The canonical
// control vector is checked through MuxedAccountFromAccountId.
func muxedAccountID(v apiVector) apiResult {
	if v.Expected.Outcome != "accept" {
		return apiResult{ID: v.ID, Status: "n/a", Detail: "the Go SDK takes a muxed ID as uint64; there is no string input to test"}
	}
	var id uint64
	if _, err := fmt.Sscanf(v.Expected.CanonicalID, "%d", &id); err != nil {
		return apiResult{ID: v.ID, Status: "defect", Detail: "runner could not read canonical_id"}
	}
	m, err := xdr.MuxedAccountFromAccountId(v.Input.Account, id)
	if err != nil {
		return apiResult{ID: v.ID, Status: "defect", Detail: "rejected a canonical ID: " + err.Error()}
	}
	if got := m.Address(); got != v.Expected.Address {
		return apiResult{ID: v.ID, Status: "defect", Detail: fmt.Sprintf("address %s, expected %s", got, v.Expected.Address)}
	}
	return apiResult{ID: v.ID, Status: "ok"}
}

func runAPI(corpus, out, version string) error {
	data, err := os.ReadFile(filepath.Join(corpus, "api-vectors.json"))
	if err != nil {
		return err
	}
	var doc struct {
		Vectors []apiVector `json:"vectors"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		return err
	}
	results := make([]apiResult, 0, len(doc.Vectors))
	fmt.Printf("go api %s %s\n", sdkModule, version)
	for _, v := range doc.Vectors {
		var r apiResult
		switch v.Kind {
		case "liquidity_pool_asset":
			r = liquidityPoolAsset(v)
		case "muxed_account_id":
			r = muxedAccountID(v)
		default:
			r = apiResult{ID: v.ID, Status: "n/a", Detail: "kind " + v.Kind + " not implemented by this runner"}
		}
		results = append(results, r)
		fmt.Printf("  %-32s %s %s\n", r.ID, r.Status, r.Detail)
	}
	report := map[string]any{
		"runner": "go", "suite": "api", "sdk": sdkModule, "sdk_version": version,
		"runtime": runtime.Version(), "results": results,
	}
	text, _ := json.MarshalIndent(report, "", "  ")
	return os.WriteFile(filepath.Join(out, "go-api-"+version+".json"), append(text, '\n'), 0o644)
}
