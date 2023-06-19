import requests
import json

checkpoint_slot = 6681760
checkpoint = '0x51cd6fb4ee0efd15c8ec91e50226ee8aeca2d5bc31422cd69a8a9acf9660318e'

rpc = 'https://www.lightclientdata.org'

bootstrap_req_url = '{}/eth/v1/beacon/light_client/bootstrap/{}'.format(rpc, checkpoint)

bootstrap_resp = requests.get(bootstrap_req_url)

with open('./mock_data/bootstrap.json', 'w') as f:
    data = bootstrap_resp.json()
    f.write(json.dumps(data, indent=4))

period = int(checkpoint_slot / 32 / 256 + 1)
count = 128
updates_req_url = '{}/eth/v1/beacon/light_client/updates?start_period={}&count={}'.format(rpc, period, count)

updates_resp= requests.get(updates_req_url)

with open('./mock_data/updates.json', 'w') as f:
    data = updates_resp.json()
    f.write(json.dumps(data, indent=4))

finlaity_update_url = '{}/eth/v1/beacon/light_client/finality_update'.format(rpc)

fin_upds = []
while len(fin_upds) < 3:
    fin_upd = requests.get(finlaity_update_url).json()
    if fin_upds:
        last_fin_upd = fin_upds[-1]
        if fin_upd['data']['finalized_header']['beacon']['slot'] == last_fin_upd['data']['finalized_header']['beacon']['slot']:
            continue
    fin_upds.append(fin_upd)
    with open(f'./mock_data/finality_update{len(fin_upds)}.json', 'w') as f:
        f.write(json.dumps(fin_upd, indent=4))

end_slot = int(fin_upds[-1]['data']['finalized_header']['beacon']['slot'])
start_slot = int(fin_upds[0]['data']['finalized_header']['beacon']['slot'])

for slot in range(start_slot, end_slot):
    req = '{}/eth/v1/beacon/headers/{}'.format(rpc, slot)
    data = requests.get(req).json()
    with open('./mock_data/headers/{}.json'.format(slot), 'w') as f:
        f.write(json.dumps(data, indent=4))
