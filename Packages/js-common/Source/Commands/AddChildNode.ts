import {Assert, E} from "web-vcore/nm/js-vextensions.js";
import {AssertV, AssertValidate, Command, CommandMeta, DBHelper, dbp, DeriveJSONSchema, GenerateUUID, SimpleSchema} from "web-vcore/nm/mobx-graphlink.js";
import {CommandRunMeta} from "../CommandMacros/CommandRunMeta.js";
import {MapEdit} from "../CommandMacros/MapEdit.js";
import {UserEdit} from "../CommandMacros/UserEdit.js";
import {AddArgumentAndClaim, TransferNodes} from "../Commands.js";
import {NodeChildLink} from "../DB/nodeChildLinks/@NodeChildLink.js";
import {GetNode} from "../DB/nodes.js";
import {MapNode, Polarity} from "../DB/nodes/@MapNode.js";
import {MapNodeRevision} from "../DB/nodes/@MapNodeRevision.js";
import {MapNodeType} from "../DB/nodes/@MapNodeType.js";
import {AddNode} from "./AddNode.js";
import {LinkNode} from "./LinkNode.js";
import {LinkNode_HighLevel} from "./LinkNode_HighLevel.js";

@MapEdit
@UserEdit
@CommandRunMeta({
	record: true,
	canShowInStream: true,
	rlsTargetPaths: [
		{table: "nodes", fieldPath: ["payload", "parentID"]},
		{table: "nodes", fieldPath: ["returnData", "nodeID"]},
	],
})
@CommandMeta({
	payloadSchema: ()=>SimpleSchema({
		mapID: {$ref: "UUID"},
		$parentID: {type: ["null", "string"]},
		$node: {$ref: "MapNode_Partial"},
		$revision: {$ref: "MapNodeRevision_Partial"},
		//link: {$ref: NodeChildLink.name},
		// todo: clean up handling of "link" field (it's marked optional, yet error occurs if left out, due to child-group not being set; and this line doesn't make enough of link's fields optional)
		link: DeriveJSONSchema(NodeChildLink, {makeOptional: ["parent", "child"]}),
		asMapRoot: {type: "boolean"},
	}),
	returnSchema: ()=>SimpleSchema({
		$nodeID: {$ref: "UUID"},
		$revisionID: {$ref: "UUID"},
		$linkID: {$ref: "UUID"},
		$doneAt: {type: "number"},
	}),
})
export class AddChildNode extends Command<{mapID?: string|n, parentID: string, node: MapNode, revision: MapNodeRevision, link?: NodeChildLink}, {nodeID: string, revisionID: string, linkID: string, doneAt: number}> {
	// controlled by parent
	recordAsNodeEdit = true;

	sub_addNode: AddNode;
	sub_addLink: LinkNode;
	parent_oldData: MapNode|n;
	Validate() {
		const {mapID, parentID, node, revision} = this.payload;
		//const link = this.payload.link = this.payload.link ?? {} as NodeChildLink;
		this.payload.link = E(new NodeChildLink(), this.payload.link);
		this.payload.link.parent = parentID;

		// this.parent_oldChildrenOrder = await GetDataAsync('nodes', parentID, '.childrenOrder') as number[];
		this.parent_oldData =
			this.Up(AddArgumentAndClaim)?.Check(a=>a.sub_addClaim == this)?.payload.argumentNode
			?? this.Up(TransferNodes)?.Check(a=>a.transferData[1]?.addNodeCommand == this)?.transferData[0].addNodeCommand?.payload.node
			?? this.Up(LinkNode_HighLevel)?.Check(a=>a.sub_addArgumentWrapper == this)?.newParent_data
			?? GetNode.NN(parentID)!;

		this.IntegrateSubcommand(()=>this.sub_addNode, null, ()=>new AddNode({mapID, node, revision}), a=>a.recordAsNodeEdit = this.recordAsNodeEdit);
		this.payload.link.child = this.sub_addNode.payload.node.id;

		this.IntegrateSubcommand(()=>this.sub_addLink, null, ()=>new LinkNode({mapID, link: this.payload.link!}));

		this.returnData = {
			nodeID: this.sub_addNode.payload.node.id,
			revisionID: this.sub_addNode.sub_addRevision.payload.revision.id,
			linkID: this.sub_addLink.payload.link.id!,
			doneAt: Date.now(),
		};
	}

	DeclareDBUpdates(db: DBHelper) {
		db.add(this.sub_addNode.GetDBUpdates(db));
		db.add(this.sub_addLink.GetDBUpdates(db));
	}
}