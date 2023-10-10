# Forcerelay/Axon Contracts Deployment

usage:
```bash
upload.sh connection https://testnet.ckbapp.dev/ <ckb_address>

upload.sh channel https://testnet.ckbapp.dev/ <ckb_address>

upload.sh packet https://testnet.ckbapp.dev/ <ckb_address>

upload.sh sudt https://testnet.ckbapp.dev/ <ckb_address>
```

before using this bash script, please modify `lock_args` in the `<contract>.deployment.file` to yours, and make sure the corresponding private key has imported into your local `ckb-cli`