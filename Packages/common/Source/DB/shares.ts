import {GetDoc, CreateAccessor, GetDocs} from "web-vcore/nm/mobx-graphlink.js";
import {Share} from "./shares/@Share.js";

export const GetShare = CreateAccessor(c=>(id: string): Share|n=>{
	if (id == null) return null;
	return GetDoc({}, a=>a.shares.get(id));
});
export const GetShares = CreateAccessor(c=>(userID: string, mapID?: string): Share[]=>{
	return GetDocs({
		/*queryOps: [
			new WhereOp("creator", "==", userID),
			mapID && new WhereOp("mapID", "==", mapID),
		].filter(a=>a),*/
		params: {filter: {
			creator: {equalTo: userID},
			mapID: mapID && {equalTo: mapID},
		}},
	}, a=>a.shares);
});