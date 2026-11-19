# capture = 'stderr'
# command = ['eval']
let port = fun base => base + 1 in
(port & (fun base => base + 2)) 8000
