#!/usr/bin/env python3
"""Prove the packaging gate rejects missing keys, broken parameters and UI literals."""
import json,shutil,subprocess,tempfile
from pathlib import Path
source=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='reader-i18n-gate-') as temp:
 root=Path(temp)
 for folder in ['scripts','core/src/i18n','ios/Reader','desktop/src','android/app/src/main/java']:
  shutil.copytree(source/folder,root/folder)
 def check(expected):
  result=subprocess.run(['python3',str(root/'scripts/check-i18n.py')],capture_output=True,text=True)
  assert (result.returncode==0)==expected,result.stdout+result.stderr
 check(True)
 file=root/'core/src/i18n/en.json';original=file.read_text();catalog=json.loads(original)
 del catalog['common.cancel'];file.write_text(json.dumps(catalog));check(False)
 file.write_text(original);catalog=json.loads(original);catalog['share.pin']='PIN: {1}';file.write_text(json.dumps(catalog));check(False)
 file.write_text('{"common.cancel":"Duplicate",'+original[1:]);check(False)
 file.write_text(original)
 test=root/'ios/Reader/UntranslatedGateFixture.swift';test.write_text('let label = Text("Untranslated button")');check(False)
 test.write_text('let label = Text("漏译按钮")');check(False)
 test.write_text('let label = L("common.missing_gate_fixture")');check(False)
 test.write_text('let label = I18n.t(if selected { "common.cancel" } else { "common.missing_gate_fixture" })');check(False)
print('PASS: clean source plus seven rejected catalog/source regressions')
