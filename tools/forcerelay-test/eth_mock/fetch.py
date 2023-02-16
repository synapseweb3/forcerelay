import requests
import json
import time

checkpoint_slot = 5787808
checkpoint = '0xe06056afdb9a0a9fd7fbaf89bb0e96eced24de0104bc5b7e3960c115d6990f90'

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


finality_update = requests.get(finlaity_update_url).json()
with open('./mock_data/finality_update1.json', 'w') as f:
    f.write(json.dumps(finality_update, indent=4))

time.sleep(30)
data = requests.get(finlaity_update_url).json()

while data['data']['finalized_header']['slot'] == finality_update['data']['finalized_header']['slot']:
    with open('./mock_data/finality_update1.json', 'w') as f:
        f.write(json.dumps(data, indent=4))
    time.sleep(30)
    data = requests.get(finlaity_update_url).json()

with open('./mock_data/finality_update2.json', 'w') as f:
    f.write(json.dumps(data, indent=4))

end_slot = int(data['data']['finalized_header']['slot'])
start_slot = int(finality_update['data']['finalized_header']['slot'])

for slot in range(start_slot, end_slot):
    req = '{}/eth/v1/beacon/headers/{}'.format(rpc, slot)
    data = requests.get(req).json()
    with open('./mock_data/headers/{}.json'.format(slot), 'w') as f:
        f.write(json.dumps(data, indent=4))
