const fs = require('fs');
const path = require('path');
const {spawnSync} = require('child_process');
const assert = require('assert');

async function main() {
    const wasmPath = process.env.HYBRID_WASM_PATH || path.join(__dirname, 'compiler/target/wasm32-unknown-unknown/release/hy_wasm.wasm');
    const nativePath = process.env.HYBRID_NATIVE_PATH || path.join(__dirname, 'compiler/target/release/hyb');
    const wasmBytes = fs.readFileSync(wasmPath);

    console.log(`Loaded WASM: ${wasmBytes.length} bytes (${(wasmBytes.length / 1024).toFixed(1)} KB)`);

    const tInit0 = performance.now();
    const wasmModule = await WebAssembly.instantiate(wasmBytes, {});
    const tInit1 = performance.now();
    console.log(`WASM instantiation time: ${(tInit1 - tInit0).toFixed(2)} ms`);

    const { hy_wasm_alloc, hy_wasm_free, hy_wasm_check, memory } = wasmModule.instance.exports;

    function checkSource(srcText) {
        const encoder = new TextEncoder();
        const encoded = encoder.encode(srcText);

        const srcPtr = hy_wasm_alloc(encoded.length);
        const memU8 = new Uint8Array(memory.buffer);
        memU8.set(encoded, srcPtr);

        const packed = BigInt(hy_wasm_check(srcPtr, encoded.length));
        hy_wasm_free(srcPtr, encoded.length);

        const outPtr = Number(packed >> 32n);
        const outLen = Number(packed & 0xFFFFFFFFn);

        const outSlice = new Uint8Array(memory.buffer, outPtr, outLen);
        const decoder = new TextDecoder();
        // Decode before returning ownership of the output buffer to the guest.
        let jsonStr;
        try {
            jsonStr = decoder.decode(outSlice);
        } finally {
            hy_wasm_free(outPtr, outLen);
        }
        return JSON.parse(jsonStr);
    }

    // Test a sample accept file
    const sampleAccept = fs.readFileSync(path.join(__dirname, 'tests/checker-corpus/16-move-send-clean.hy'), 'utf8');
    const resAccept = checkSource(sampleAccept);
    console.log('Sample Accept verdict:', resAccept.summary.verdict, 'errors:', resAccept.summary.errors);

    // Test a sample reject file
    const sampleReject = fs.readFileSync(path.join(__dirname, 'tests/checker-corpus/01-shared-map-across-tasks.hy'), 'utf8');
    const resReject = checkSource(sampleReject);
    console.log('Sample Reject verdict:', resReject.summary.verdict, 'errors:', resReject.summary.errors, 'rule:', resReject.diagnostics[0]?.tier0_rule);

    // Test across all corpus files and measure time
    const corpusDir = path.join(__dirname, 'tests/checker-corpus');
    const files = fs.readdirSync(corpusDir).filter(f => f.endsWith('.hy')).sort();

    console.log(`\nVerifying all ${files.length} corpus files against WASM...`);
    let passed = 0;
    let nativeMatches = 0;
    const times = [];

    for (const f of files) {
        const fullPath = path.join(corpusDir, f);
        const src = fs.readFileSync(fullPath, 'utf8');
        const m = src.match(/^\/\/\s*EXPECTED-VERDICT:\s*(\S+)\b\s*(\S*)/m);
        const expectedVerdict = m ? m[1] : null;
        const expectedRule = (m && m[2]) ? m[2] : null;

        const t0 = performance.now();
        const res = checkSource(src);
        const t1 = performance.now();
        times.push(t1 - t0);

        // Compare complete parsed envelopes after normalizing only the input
        // filename (the WASM ABI currently calls its input "input.hy").
        const native = spawnSync(nativePath, ['check', fullPath, '--json'], {encoding:'utf8'});
        if (native.error || ![0,1].includes(native.status)) throw native.error || new Error(`native CLI failed: ${native.status}: ${native.stderr}`);
        const nativeOutput = JSON.parse(native.stdout);
        function normalize(value) {
            if (typeof value === 'string') return value.split(fullPath).join('input.hy');
            if (Array.isArray(value)) return value.map(normalize);
            if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(k => [k, normalize(value[k])]));
            return value;
        }
        assert.deepStrictEqual(normalize(res), normalize(nativeOutput), `native/WASM diagnostic mismatch: ${f}`);
        nativeMatches++;

        const verdict = res.summary ? res.summary.verdict : res.verdict;
        const errors = res.summary ? res.summary.errors : res.errors;

        let match = false;
        if (expectedVerdict === 'ACCEPT') {
            match = verdict === 'accept' && errors === 0;
        } else if (expectedVerdict === 'ACCEPT-MANAGED') {
            match = verdict === 'accept' && errors === 0;
        } else if (expectedVerdict === 'REJECT') {
            const firstErr = res.diagnostics.find(d => d.severity === 'error');
            const ruleOk = expectedRule ? (firstErr && firstErr.tier0_rule === expectedRule) : true;
            match = verdict === 'reject' && ruleOk;
        }

        if (match) {
            passed++;
        } else {
            console.error(`FAIL ${f}: expected ${expectedVerdict} (${expectedRule}), got ${res.verdict}`);
        }
    }

    times.sort((a, b) => a - b);
    const mean = times.reduce((a, b) => a + b, 0) / times.length;
    const p50 = times[Math.floor(times.length / 2)];
    const p90 = times[Math.floor(times.length * 0.9)];
    const p99 = times[times.length - 1];

    console.log(`Corpus fixture agreement: ${passed}/${files.length}`);
    console.log(`Native/WASM normalized envelope parity: ${nativeMatches}/${files.length}`);
    if (files.length === 0 || passed !== files.length || nativeMatches !== files.length) process.exitCode = 1;
    console.log(`WASM Check Latency per file:`);
    console.log(`  mean: ${mean.toFixed(3)} ms`);
    console.log(`  p50:  ${p50.toFixed(3)} ms`);
    console.log(`  p90:  ${p90.toFixed(3)} ms`);
    console.log(`  p99:  ${p99.toFixed(3)} ms`);
}

main().catch(err => {
    console.error(err);
    process.exit(1);
});
